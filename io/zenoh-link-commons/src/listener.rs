//
// Copyright (c) 202 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use async_std::net::SocketAddr;
use async_std::task;
use async_std::task::JoinHandle;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use zenoh_core::{zread, zwrite};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::Signal;

pub struct UnicastListener {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl UnicastListener {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> UnicastListener {
        UnicastListener {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct UnicastListeners {
    listeners: Arc<RwLock<HashMap<SocketAddr, UnicastListener>>>,
}

impl UnicastListeners {
    pub fn new() -> UnicastListeners {
        UnicastListeners {
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_listener(
        &self,
        endpoint: EndPoint,
        addr: SocketAddr,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ZResult<()> {
        let mut listeners = zwrite!(self.listeners);
        let c_listeners = self.listeners.clone();
        let c_addr = addr;
        let wraphandle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = handle.await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        let listener = UnicastListener::new(endpoint, active, signal, wraphandle);
        // Update the list of active listeners on the manager
        listeners.insert(addr, listener);
        Ok(())
    }

    pub async fn del_listener(&self, addr: SocketAddr) -> ZResult<()> {
        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            zerror!(
                "Can not delete the listener because it has not been found: {}",
                addr
            )
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        listener.handle.await
    }

    pub fn get_endpoints(&self) -> Vec<EndPoint> {
        zread!(self.listeners)
            .values()
            .map(|l| l.endpoint.clone())
            .collect()
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            let (kip, kpt) = (key.ip(), key.port());

            // Either ipv4/0.0.0.0 or ipv6/[::]
            if kip.is_unspecified() {
                let mut addrs = match kip {
                    IpAddr::V4(_) => zenoh_util::net::get_ipv4_ipaddrs(),
                    IpAddr::V6(_) => zenoh_util::net::get_ipv6_ipaddrs(),
                };
                let iter = addrs.drain(..).map(|x| {
                    Locator::new(
                        value.endpoint.protocol(),
                        SocketAddr::new(x, kpt).to_string(),
                        value.endpoint.metadata(),
                    )
                    .unwrap()
                });
                locators.extend(iter);
            } else {
                locators.push(value.endpoint.to_locator());
            }
        }

        locators
    }
}
