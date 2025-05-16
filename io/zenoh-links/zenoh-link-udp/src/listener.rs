//
// Copyright (c) 2024 ZettaScale Technology
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
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
};

use flume::Sender;
use futures::Future;
use tokio_util::sync::CancellationToken;
use zenoh_core::{zread, zwrite};
use zenoh_link_commons::BIND_INTERFACE;
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{zerror, ZResult};

pub struct ListenerUnicastUDP {
    endpoint: EndPoint,
    sender: Sender<()>,
}

impl ListenerUnicastUDP {
    fn new(endpoint: EndPoint, sender: Sender<()>) -> ListenerUnicastUDP {
        ListenerUnicastUDP { endpoint, sender }
    }

    async fn stop(&self) {
        self.sender.send_async(()).await.ok();
    }
}

pub struct ListenersUnicastUDP {
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastUDP>>>,
    pub listeners_cancellation_token: CancellationToken,
}

impl ListenersUnicastUDP {
    pub fn new() -> ListenersUnicastUDP {
        ListenersUnicastUDP {
            listeners: Arc::new(RwLock::new(HashMap::new())),
            listeners_cancellation_token: CancellationToken::new(),
        }
    }

    pub async fn add_listener<F>(
        &self,
        endpoint: EndPoint,
        addr: SocketAddr,
        future: F,
        sender: flume::Sender<()>,
    ) -> ZResult<()>
    where
        F: Future<Output = ZResult<()>> + Send + 'static,
    {
        let mut listeners = zwrite!(self.listeners);
        let c_listeners = self.listeners.clone();
        let c_addr = addr;
        let task = async move {
            // Wait for the accept loop to terminate
            let res = future.await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        };
        drop(zenoh_runtime::ZRuntime::Acceptor.spawn(task));

        let listener = ListenerUnicastUDP::new(endpoint, sender);
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
        listener.stop().await;
        Ok(())
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
            let config = value.endpoint.config();
            let iface = config.get(BIND_INTERFACE);

            // Either ipv4/0.0.0.0 or ipv6/[::]
            if kip.is_unspecified() {
                let mut addrs = match kip {
                    IpAddr::V4(_) => zenoh_util::net::get_ipv4_ipaddrs(iface),
                    IpAddr::V6(_) => zenoh_util::net::get_ipv6_ipaddrs(iface),
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

impl Default for ListenersUnicastUDP {
    fn default() -> Self {
        Self::new()
    }
}
