//
// Copyright (c) 2023 ZettaScale Technology
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
use async_std::task::JoinHandle;
use async_std::task::{self};
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{zasynclock, zread, zwrite};
use zenoh_link_commons::{
    ConstructibleLinkManagerUnicast, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::Signal;

use z_serial::ZSerial;

use crate::get_exclusive;

use super::{
    get_baud_rate, get_unix_path_as_string, SERIAL_ACCEPT_THROTTLE_TIME, SERIAL_DEFAULT_MTU,
    SERIAL_LOCATOR_PREFIX,
};

struct LinkUnicastSerial {
    // The underlying serial port as returned by ZSerial (tokio-serial)
    // NOTE: ZSerial requires &mut for read and write operations. This means
    //       that concurrent reads and writes are not possible. To achieve that,
    //       we use an UnsafeCell for interior mutability. Using an UnsafeCell
    //       is safe in our case since the transmission and reception logic
    //       already ensures that no concurrent reads or writes can happen on
    //       the same stream: there is only one task at the time that writes on
    //       the stream and only one task at the time that reads from the stream.
    port: UnsafeCell<ZSerial>,
    // The serial port path
    src_locator: Locator,
    // The serial destination path (random UUIDv4)
    dst_locator: Locator,
    // A flag that tells if the link is connected or not
    is_connected: Arc<AtomicBool>,
    // Locks for reading and writing ends of the serial.
    write_lock: AsyncMutex<()>,
    read_lock: AsyncMutex<()>,
}

unsafe impl Send for LinkUnicastSerial {}
unsafe impl Sync for LinkUnicastSerial {}

impl LinkUnicastSerial {
    fn new(
        port: UnsafeCell<ZSerial>,
        src_path: &str,
        dst_path: &str,
        is_connected: Arc<AtomicBool>,
    ) -> Self {
        Self {
            port,
            src_locator: Locator::new(SERIAL_LOCATOR_PREFIX, src_path, "").unwrap(),
            dst_locator: Locator::new(SERIAL_LOCATOR_PREFIX, dst_path, "").unwrap(),
            is_connected,
            write_lock: AsyncMutex::new(()),
            read_lock: AsyncMutex::new(()),
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The write_lock and read_lock
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_port_mut(&self) -> &mut ZSerial {
        unsafe { &mut *self.port.get() }
    }

    fn is_ready(&self) -> bool {
        let res = match self.get_port_mut().bytes_to_read() {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    "Unable to check if there are bytes to read in serial {}: {}",
                    self.src_locator,
                    e
                );
                0
            }
        };
        if res > 0 {
            return true;
        }
        false
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastSerial {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing Serial link: {}", self);
        let _guard = zasynclock!(self.write_lock);
        self.get_port_mut().clear().map_err(|e| {
            let e = zerror!("Unable to close Serial link {}: {}", self, e);
            log::error!("{}", e);
            e
        })?;
        self.is_connected.store(false, Ordering::Release);
        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_lock);
        self.get_port_mut().write(buffer).await.map_err(|e| {
            let e = zerror!("Unable to write on Serial link {}: {}", self, e);
            log::error!("{}", e);
            e
        })?;
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
        loop {
            let _guard = zasynclock!(self.read_lock);
            match self.get_port_mut().read_msg(buffer).await {
                Ok(read) => return Ok(read),
                Err(e) => {
                    let e = zerror!("Read error on Serial link {}: {}", self, e);
                    log::error!("{}", e);
                    drop(_guard);
                    async_std::task::sleep(std::time::Duration::from_millis(1)).await;
                    continue;
                }
            }
        }
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
        false
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
        let path = get_unix_path_as_string(endpoint.address());
        let baud_rate = get_baud_rate(&endpoint);
        let exclusive = get_exclusive(&endpoint);
        log::trace!("Opening Serial Link on device {path:?}, with baudrate {baud_rate} and exclusive set as {exclusive}");
        let port = ZSerial::new(path.clone(), baud_rate, exclusive).map_err(|e| {
            let e = zerror!(
                "Can not create a new Serial link bound to {:?}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        // Create Serial link
        let link = Arc::new(LinkUnicastSerial::new(
            UnsafeCell::new(port),
            &path,
            &path,
            Arc::new(AtomicBool::new(true)),
        ));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let path = get_unix_path_as_string(endpoint.address());
        let baud_rate = get_baud_rate(&endpoint);
        let exclusive = get_exclusive(&endpoint);
        log::trace!("Creating Serial listener on device {path:?}, with baudrate {baud_rate} and exclusive set as {exclusive}");
        let port = ZSerial::new(path.clone(), baud_rate, exclusive).map_err(|e| {
            let e = zerror!(
                "Can not create a new Serial link bound to {:?}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        // Creating the link
        let is_connected = Arc::new(AtomicBool::new(false));
        let dst_path = format!("{}", uuid::Uuid::new_v4());
        let link = Arc::new(LinkUnicastSerial::new(
            UnsafeCell::new(port),
            &path,
            &dst_path,
            is_connected.clone(),
        ));

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();
        let mut listeners = zwrite!(self.listeners);

        let c_path = path.clone();
        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_read_task(
                link,
                c_active,
                c_signal,
                c_manager,
                c_path.clone(),
                is_connected,
            )
            .await;
            zwrite!(c_listeners).remove(&c_path);
            res
        });

        let locator = endpoint.to_locator();
        let listener = ListenerUnicastSerial::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        listeners.insert(path, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let path = get_unix_path_as_string(endpoint.address());

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
            .map(|x| x.endpoint.to_locator())
            .collect()
    }
}

async fn accept_read_task(
    link: Arc<LinkUnicastSerial>,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
    src_path: String,
    is_connected: Arc<AtomicBool>,
) -> ZResult<()> {
    enum Action {
        Receive(Arc<LinkUnicastSerial>),
        Stop,
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    async fn receive(
        link: Arc<LinkUnicastSerial>,
        active: Arc<AtomicBool>,
        src_path: String,
        is_connected: Arc<AtomicBool>,
    ) -> ZResult<Action> {
        while active.load(Ordering::Acquire) {
            if !is_connected.load(Ordering::Acquire) {
                if !link.is_ready() {
                    // Waiting to be ready, if not sleep some time.
                    task::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }

                log::trace!("Creating serial link from {:?}", src_path);

                is_connected.store(true, Ordering::Release);

                return Ok(Action::Receive(link.clone()));
            }
        }
        Ok(Action::Stop)
    }

    log::trace!("Ready to accept Serial connections on: {:?}", src_path);

    loop {
        match receive(
            link.clone(),
            active.clone(),
            src_path.clone(),
            is_connected.clone(),
        )
        .race(stop(signal.clone()))
        .await
        {
            Ok(action) => match action {
                Action::Receive(link) => {
                    // Communicate the new link to the initial transport manager
                    if let Err(e) = manager.send_async(LinkUnicast(link.clone())).await {
                        log::error!("{}-{}: {}", file!(), line!(), e)
                    }
                }
                Action::Stop => break Ok(()),
            },
            Err(e) => {
                log::warn!("{}. Hint: Is the serial cable connected?", e);
                task::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
                continue;
            }
        }
    }
}
