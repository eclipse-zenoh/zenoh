//
// Copyright (c) 2022 ZettaScale Technology
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
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;
use zenoh_core::Result as ZResult;
use zenoh_core::{bail, zasynclock, zerror, zlock, zread, zwrite};
use zenoh_link_commons::{
    ConstructibleLinkManagerUnicast, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    NewLinkChannelSender,
};
use zenoh_protocol_core::{EndPoint, Locator};
use zenoh_sync::{Mvar, Signal};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{DataBits, FlowControl, Parity, SerialPort, SerialPortBuilderExt, SerialStream, ClearBuffer, StopBits};


use super::{get_unix_path, get_unix_path_as_string, SERIAL_DEFAULT_MTU, SERIAL_LOCATOR_PREFIX};





struct LinkUnicastSerial {
    // The underlying serial port
    port: Arc<AsyncMutex<SerialStream>>,
    // The serial port path
    src_locator: Locator,
    // The serial destination path (random UUIDv4)
    dst_locator: Locator,
    // A flag that tells if the link is connected or not
    is_connected: Arc<AtomicBool>,
}

impl LinkUnicastSerial {
    fn new(port: Arc<AsyncMutex<SerialStream>>, src_path: &str, dst_path: &str, is_connected: Arc<AtomicBool>,) -> Self {
        Self {
            port,
            src_locator: Locator::new(SERIAL_LOCATOR_PREFIX, &src_path),
            dst_locator: Locator::new(SERIAL_LOCATOR_PREFIX, &dst_path),
            is_connected,
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastSerial {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing Serial link: {}", self);
        let guard = zasynclock!(self.port);
        guard.clear(ClearBuffer::All).map_err(|e| {
            let e = zerror!("Unable to close Serial link {}: {}", self, e);
            log::trace!("{}", e);
            e
        })?;
        self.is_connected.store(false, Ordering::Release);
        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        log::error!("TO SERIAL {} : {:02X?}", buffer.len(), buffer);
        let mut guard = zasynclock!(self.port);
        guard.flush().await.unwrap();
        // Ok(guard.write(buffer).await.map_err(|e| {
        //     let e = zerror!("Write error on Serial link {}: {}", self, e);
        //     log::trace!("{}", e);
        //     e
        // })?)
        guard.write_all(buffer).await.map_err(|e| {
                let e = zerror!("Write error on Serial link {}: {}", self, e);
                log::trace!("{}", e);
                e
            })?;

        log::error!("ACTUALLY WRITTEN {} bytes", buffer.len());
        guard.flush().await.unwrap();
        Ok(buffer.len())
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.port);
        // Ok(guard.read(buffer).await.map_err(|e| {
        //     let e = zerror!("Read error on Serial link {}: {}", self, e);
        //     log::trace!("{}", e);
        //     e
        // })?)
        let read = guard.read(buffer).await.map_err(|e| {
            let e = zerror!("Read error on Serial link {}: {}", self, e);
            log::trace!("{}", e);
            e
        })?;
        log::error!("FROM SERIAL: {:02X?}", &buffer[0..read]);
        Ok(read)
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut read: usize = 0;
        while read < buffer.len() {
            let n = self.read(&mut buffer[read..]).await?;
            read += n;
        }
        Ok(())
    }

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.src_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.dst_locator
    }

    #[inline(always)]
    fn get_mtu(&self) -> u16 {
        *SERIAL_DEFAULT_MTU
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        false
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }
}

impl fmt::Display for LinkUnicastSerial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_locator, self.dst_locator)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastSerial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Serial")
            .field("src", &self.src_locator)
            .field("dst", &self.dst_locator)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnicastSerial {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastSerial {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> Self {
        Self {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastSerial {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<String, ListenerUnicastSerial>>>,
}

impl LinkManagerUnicastSerial {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
impl ConstructibleLinkManagerUnicast<()> for LinkManagerUnicastSerial {
    fn new(new_link_sender: NewLinkChannelSender, _: ()) -> ZResult<Self> {
        Ok(Self::new(new_link_sender))
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastSerial {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let path = get_unix_path_as_string(&endpoint.locator);

        let port = tokio_serial::new(&path, 9_600)
            .data_bits(DataBits::Eight)
            .flow_control(FlowControl::None)
            .parity(Parity::Odd)
            .stop_bits(StopBits::Two)
            .open_native_async()
            .map_err(|e| {
                let e = zerror!(
                    "Can not create a new Serial link bound to {:?}: {}",
                    path,
                    e
                );
                log::warn!("{}", e);
                e
            })?;

        // Create Serial link
        let link = Arc::new(LinkUnicastSerial::new(Arc::new(AsyncMutex::new(port)), &path, &path,  Arc::new(AtomicBool::new(true))));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let path = get_unix_path_as_string(&endpoint.locator);

        // Open the serial port
        let port = tokio_serial::new(&path, 9_600)
            .data_bits(DataBits::Eight)
            .flow_control(FlowControl::None)
            .parity(Parity::Odd)
            .stop_bits(StopBits::Two)
            .open_native_async()
            .map_err(|e| {
                let e = zerror!(
                    "Can not create a new Serial link bound to {:?}: {}",
                    path,
                    e
                );
                log::warn!("{}", e);
                e
            })?;

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();
        let c_path = path.clone();
        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_read_task(
                Arc::new(AsyncMutex::new(port)),
                c_active,
                c_signal,
                c_manager,
                c_path.clone(),
            )
            .await;
            zwrite!(c_listeners).remove(&c_path);
            res
        });

        let locator = endpoint.locator.clone();
        let listener = ListenerUnicastSerial::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(path, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let path = get_unix_path_as_string(&endpoint.locator);

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&path).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the Serial listener because it has not been found: {}",
                path
            );
            log::trace!("{}", e);
            e
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        listener.handle.await
    }

    fn get_listeners(&self) -> Vec<EndPoint> {
        zread!(self.listeners)
            .values()
            .map(|l| l.endpoint.clone())
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        zread!(self.listeners)
            .values()
            .map(|x| x.endpoint.locator.clone())
            .collect()
    }
}

async fn accept_read_task(
    port: Arc<AsyncMutex<SerialStream>>,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
    src_path: String,
) -> ZResult<()> {
    enum Action {
        Receive(SerialStream),
        Stop,
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let is_connected = Arc::new(AtomicBool::new(false));

    log::trace!("Ready to accept Serial connections on: {:?}", src_path);

    while active.load(Ordering::Acquire) {
        if !is_connected.load(Ordering::Acquire) {
            let c_port  = port.clone();
            let guard = zasynclock!(c_port);

            if guard.bytes_to_read().unwrap() > 0 {
                log::trace!("Ready to read from Serial connection on {:?}", src_path);
                // Create the new link object
                let dst_path = format!("{}", uuid::Uuid::new_v4());

                let link = Arc::new(LinkUnicastSerial::new(port.clone(), &src_path, &dst_path, is_connected.clone()));
                is_connected.store(true, Ordering::Release);

                log::trace!("Creating new link from {:?}", src_path);

                // Communicate the new link to the initial transport manager
                if let Err(e) = manager.send_async(LinkUnicast(link)).await {
                    log::error!("{}-{}: {}", file!(), line!(), e)
                }
            }
        } else {
            ()
            //task::sleep(Duration::from_secs(10)).await;
        }
    }

    Ok(())
}
