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

use std::{
    cell::UnsafeCell,
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::{
    sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use z_serial::ZSerial;
use zenoh_core::{bail, zasynclock, zasyncread, zasyncwrite};
use zenoh_link_commons::{
    ConstructibleLinkManagerUnicast, LinkAuthId, LinkManagerUnicastTrait, LinkUnicast,
    LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{zerror, ZResult};

use super::{
    get_baud_rate, get_unix_path_as_string, SERIAL_ACCEPT_THROTTLE_TIME, SERIAL_DEFAULT_MTU,
    SERIAL_LOCATOR_PREFIX,
};
use crate::{get_exclusive, get_release_on_close, get_timeout};

struct LinkUnicastSerial {
    // The underlying serial port as returned by ZSerial (tokio-serial)
    // NOTE: ZSerial requires &mut for read and write operations. This means
    //       that concurrent reads and writes are not possible. To achieve that,
    //       we use an UnsafeCell for interior mutability. Using an UnsafeCell
    //       is safe in our case since the transmission and reception logic
    //       already ensures that no concurrent reads or writes can happen on
    //       the same stream: there is only one task at the time that writes on
    //       the stream and only one task at the time that reads from the stream.
    port: UnsafeCell<Option<ZSerial>>,
    // The serial port path
    src_locator: Locator,
    // The serial destination path (random UUIDv4)
    dst_locator: Locator,
    // A flag that tells if the link is connected or not
    is_connected: Arc<AtomicBool>,
    // A flag that tells if we must release the file on close.
    release_on_close: bool,
    // Locks for reading and writing ends of the serial.
    write_lock: AsyncMutex<()>,
    read_lock: AsyncMutex<()>,
}

unsafe impl Send for LinkUnicastSerial {}
unsafe impl Sync for LinkUnicastSerial {}

impl LinkUnicastSerial {
    fn new(
        port: UnsafeCell<Option<ZSerial>>,
        src_path: &str,
        dst_path: &str,
        is_connected: Arc<AtomicBool>,
        release_on_close: bool,
    ) -> Self {
        Self {
            port,
            src_locator: Locator::new(SERIAL_LOCATOR_PREFIX, src_path, "").unwrap(),
            dst_locator: Locator::new(SERIAL_LOCATOR_PREFIX, dst_path, "").unwrap(),
            is_connected,
            release_on_close,
            write_lock: AsyncMutex::new(()),
            read_lock: AsyncMutex::new(()),
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The write_lock and read_lock
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_port_mut(&self) -> ZResult<&mut ZSerial> {
        unsafe {
            let opt = &mut *self.port.get();

            if let Some(port) = opt {
                return Ok(port);
            }
            bail!("Serial is not opened")
        }
    }

    fn clear_buffers(&self) -> ZResult<()> {
        tracing::trace!("I'm cleaning the buffers");
        Ok(self
            .get_port_mut()?
            .clear()
            .map_err(|e| zerror!("Cannot clear serial buffers: {e:?}"))?)
    }

    fn set_port(&self, port: ZSerial) {
        unsafe { *self.port.get() = Some(port) }
    }

    fn unset_port(&self) {
        unsafe { *self.port.get() = None }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastSerial {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing Serial link: {}", self);
        let _guard = zasynclock!(self.write_lock);
        self.is_connected.store(false, Ordering::Release);
        self.get_port_mut()?.close();
        if self.release_on_close {
            self.unset_port();
        }

        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_lock);
        self.get_port_mut()?.write(buffer).await.map_err(|e| {
            let e = zerror!("Unable to write on Serial link {}: {}", self, e);
            tracing::error!("{}", e);
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
        let _guard = zasynclock!(self.read_lock);
        match self.get_port_mut()?.read_msg(buffer).await {
            Ok(read) => return Ok(read),
            Err(e) => {
                let e = zerror!("Read error on Serial link {}: {}", self, e);
                tracing::error!("{}", e);
                drop(_guard);
                bail!("Read error on Serial link {}: {}", self, e);
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
    fn get_mtu(&self) -> BatchSize {
        *SERIAL_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        // For POSIX systems, the interface name refers to the file name without the path
        // e.g. for serial port "/dev/ttyUSB0" interface name will be "ttyUSB0"
        match z_serial::get_available_port_names() {
            Ok(interfaces) => {
                tracing::trace!("get_interface_names for serial: {:?}", interfaces);
                interfaces
            }
            Err(e) => {
                tracing::debug!("get_interface_names for serial failed: {:?}", e);
                vec![]
            }
        }
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        super::IS_RELIABLE
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        false
    }

    #[inline(always)]
    fn get_auth_id(&self) -> &LinkAuthId {
        &LinkAuthId::Serial
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
    token: CancellationToken,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastSerial {
    fn new(endpoint: EndPoint, token: CancellationToken, handle: JoinHandle<ZResult<()>>) -> Self {
        Self {
            endpoint,
            token,
            handle,
        }
    }

    async fn stop(&self) {
        self.token.cancel();
    }
}

pub struct LinkManagerUnicastSerial {
    manager: NewLinkChannelSender,
    listeners: Arc<AsyncRwLock<HashMap<String, ListenerUnicastSerial>>>,
}

impl LinkManagerUnicastSerial {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(AsyncRwLock::new(HashMap::new())),
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
        let tout = get_timeout(&endpoint);
        let release_on_close = get_release_on_close(&endpoint);
        tracing::trace!("Opening Serial Link on device {path:?}, with baudrate {baud_rate}, exclusive set as {exclusive} and timeout (us) {tout}");
        let mut port = ZSerial::new(path.clone(), baud_rate, exclusive).map_err(|e| {
            let e = zerror!(
                "Can not create a new Serial link bound to {:?}: {}",
                path,
                e
            );
            tracing::warn!("{}", e);
            e
        })?;

        // Clear buffers
        port.clear()?;
        port.connect(Some(Duration::from_micros(tout))).await?;

        // Create Serial link
        let link = Arc::new(LinkUnicastSerial::new(
            UnsafeCell::new(Some(port)),
            &path,
            &path,
            Arc::new(AtomicBool::new(true)),
            release_on_close,
        ));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let path = get_unix_path_as_string(endpoint.address());
        let baud_rate = get_baud_rate(&endpoint);
        let exclusive = get_exclusive(&endpoint);
        let release_on_close = get_release_on_close(&endpoint);

        // Creating the link
        let is_connected = Arc::new(AtomicBool::new(false));
        let dst_path = format!("{}", uuid::Uuid::new_v4());
        let link = Arc::new(LinkUnicastSerial::new(
            UnsafeCell::new(None),
            &path,
            &dst_path,
            is_connected.clone(),
            release_on_close,
        ));

        // Spawn the accept loop for the listener
        let token = CancellationToken::new();
        let mut listeners = zasyncwrite!(self.listeners);

        let task = {
            let token = token.clone();
            let path = path.clone();
            let manager = self.manager.clone();
            let listeners = self.listeners.clone();

            async move {
                // Wait for the accept loop to terminate
                let res = accept_read_task(
                    link,
                    token,
                    manager,
                    path.clone(),
                    is_connected,
                    baud_rate,
                    exclusive,
                    release_on_close,
                )
                .await;
                zasyncwrite!(listeners).remove(&path);
                res
            }
        };
        let handle = zenoh_runtime::ZRuntime::Acceptor.spawn(task);

        let locator = endpoint.to_locator();
        let listener = ListenerUnicastSerial::new(endpoint, token, handle);
        // Update the list of active listeners on the manager
        listeners.insert(path, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let path = get_unix_path_as_string(endpoint.address());

        // Stop the listener
        let listener = zasyncwrite!(self.listeners).remove(&path).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the Serial listener because it has not been found: {}",
                path
            );
            tracing::trace!("{}", e);
            e
        })?;

        // Send the stop signal
        listener.stop().await;
        listener.handle.await?
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        zasyncread!(self.listeners)
            .values()
            .map(|l| l.endpoint.clone())
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        zasyncread!(self.listeners)
            .values()
            .map(|x| x.endpoint.to_locator())
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_read_task(
    link: Arc<LinkUnicastSerial>,
    token: CancellationToken,
    manager: NewLinkChannelSender,
    src_path: String,
    is_connected: Arc<AtomicBool>,
    baud_rate: u32,
    exclusive: bool,
    release_on_close: bool,
) -> ZResult<()> {
    #[allow(clippy::too_many_arguments)]
    async fn receive(
        link: Arc<LinkUnicastSerial>,
        src_path: String,
        is_connected: Arc<AtomicBool>,
        baud_rate: u32,
        exclusive: bool,
        release_on_close: bool,
    ) -> ZResult<Arc<LinkUnicastSerial>> {
        tokio::time::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;

        while is_connected.load(Ordering::Acquire) {
            // The serial is already connected to nothing.
            tokio::time::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
        }

        tracing::trace!("Creating Serial listener on device {src_path:?}, with baudrate {baud_rate} and exclusive set as {exclusive}");
        if release_on_close {
            let port = ZSerial::new(src_path.clone(), baud_rate, exclusive).map_err(|e| {
                zerror!(
                    "Can not create a new Serial link bound to {:?}: {}",
                    src_path,
                    e
                )
            })?;

            link.set_port(port);
            // Cleaning RX buffer before listening
            link.clear_buffers()?;
        }

        while link.get_port_mut()?.accept().await.is_err() {
            //Waiting to be ready, if not sleep some time.
            tokio::time::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
        }

        tracing::trace!("Creating serial link from {:?}", src_path);
        is_connected.store(true, Ordering::Release);
        Ok(link.clone())
    }

    tracing::trace!("Ready to accept Serial connections on: {:?}", src_path);

    loop {
        if !is_connected.load(Ordering::Acquire) {
            tokio::select! {
                res = receive(
                    link.clone(),
                    src_path.clone(),
                    is_connected.clone(),
                    baud_rate,
                    exclusive,
                    release_on_close,
                ) => {
                    match res {
                        Ok(link) => {
                            // Communicate the new link to the initial transport manager
                            if let Err(e) = manager.send_async(LinkUnicast(link.clone())).await {
                                tracing::debug!("{}-{}: {}", file!(), line!(), e)
                            }

                            // Ensure the creation of this link is only once
                            continue;
                        }
                        Err(e) =>  {
                            tracing::debug!("{}. Hint: Is the serial cable connected?", e);
                            tokio::time::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
                            continue;

                        }
                    }
                },

                _ = token.cancelled() => break,
            }
        } else {
            // In this case its already connected, so we do nothing
            tokio::time::sleep(Duration::from_micros(*SERIAL_ACCEPT_THROTTLE_TIME)).await;
        }
    }
    Ok(())
}
