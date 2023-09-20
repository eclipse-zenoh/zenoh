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
use crate::config;
#[cfg(not(target_os = "macos"))]
use advisory_lock::{AdvisoryFileLock, FileLockMode};
use async_io::Async;
use async_std::fs::remove_file;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use filepath::FilePath;
use nix::unistd::unlink;
use rand::Rng;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::Arc;
use zenoh_core::{zasyncread, zasyncwrite};
use zenoh_protocol::core::{EndPoint, Locator};

use unix_named_pipe::{create, open_read, open_write};

use zenoh_link_commons::{
    ConstructibleLinkManagerUnicast, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    NewLinkChannelSender,
};
use zenoh_result::{bail, ZResult};

use super::FILE_ACCESS_MASK;

const LINUX_PIPE_MAX_MTU: u16 = 65_535;
const LINUX_PIPE_DEDICATE_TRIES: usize = 100;

static PIPE_INVITATION: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];

struct Invitation;
impl Invitation {
    async fn send(suffix: u32, pipe: &mut PipeW) -> ZResult<()> {
        let msg: [u8; 8] = {
            let mut msg: [u8; 8] = [0; 8];
            let (one, two) = msg.split_at_mut(PIPE_INVITATION.len());
            one.copy_from_slice(PIPE_INVITATION);
            two.copy_from_slice(&suffix.to_ne_bytes());
            msg
        };
        pipe.write_all(&msg).await
    }

    async fn receive(pipe: &mut PipeR) -> ZResult<u32> {
        let mut msg: [u8; 8] = [0; 8];
        pipe.read_exact(&mut msg).await?;
        if !msg.starts_with(PIPE_INVITATION) {
            bail!("Unexpected invitation received during pipe handshake!")
        }

        let suffix_bytes: &[u8; 4] = &msg[4..].try_into()?;
        let suffix = u32::from_ne_bytes(*suffix_bytes);
        Ok(suffix)
    }

    async fn confirm(suffix: u32, pipe: &mut PipeW) -> ZResult<()> {
        Self::send(suffix, pipe).await
    }

    async fn expect(expected_suffix: u32, pipe: &mut PipeR) -> ZResult<()> {
        let recived_suffix = Self::receive(pipe).await?;
        if recived_suffix != expected_suffix {
            bail!(
                "Suffix mismatch: expected {} got {}",
                expected_suffix,
                recived_suffix
            )
        }
        Ok(())
    }
}

struct PipeR {
    pipe: Async<File>,
}

impl Drop for PipeR {
    fn drop(&mut self) {
        if let Ok(path) = self.pipe.as_mut().path() {
            let _ = unlink(&path);
        }
    }
}
impl PipeR {
    async fn new(path: &str, access_mode: u32) -> ZResult<Self> {
        // create, open and lock named pipe
        let pipe_file = Self::create_and_open_unique_pipe_for_read(path, access_mode).await?;
        // create async_io wrapper for pipe's file descriptor
        let pipe = Async::new(pipe_file)?;
        Ok(Self { pipe })
    }

    async fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> ZResult<usize> {
        let result = self
            .pipe
            .read_with_mut(|pipe| match pipe.read(&mut buf[..]) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => Ok(val),
                Err(e) => Err(e),
            })
            .await?;
        ZResult::Ok(result)
    }

    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ZResult<()> {
        let mut r: usize = 0;
        self.pipe
            .read_with_mut(|pipe| match pipe.read(&mut buf[r..]) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => {
                    r += val;
                    if r == buf.len() {
                        return Ok(());
                    }
                    Err(async_std::io::ErrorKind::WouldBlock.into())
                }
                Err(e) => Err(e),
            })
            .await?;
        ZResult::Ok(())
    }

    async fn create_and_open_unique_pipe_for_read(path_r: &str, access_mode: u32) -> ZResult<File> {
        let r_was_created = create(path_r, Some(access_mode));
        let open_result = Self::open_unique_pipe_for_read(path_r);
        match (open_result.as_ref(), r_was_created) {
            (Err(_), Ok(_)) => {
                // clean-up in case of failure
                let _ = remove_file(path_r).await;
            }
            (Ok(mut pipe_file), Err(_)) => {
                // drop all the data from the pipe in case if it already exists
                let mut buf: [u8; 1] = [0; 1];
                while let Ok(val) = pipe_file.read(&mut buf) {
                    if val == 0 {
                        break;
                    }
                }
            }
            _ => {}
        }

        open_result
    }

    fn open_unique_pipe_for_read(path: &str) -> ZResult<File> {
        let read = open_read(path)?;
        #[cfg(not(target_os = "macos"))]
        read.try_lock(FileLockMode::Exclusive)?;
        Ok(read)
    }
}

struct PipeW {
    pipe: Async<File>,
}
impl PipeW {
    async fn new(path: &str) -> ZResult<Self> {
        // create, open and lock named pipe
        let pipe_file = Self::open_unique_pipe_for_write(path)?;
        // create async_io wrapper for pipe's file descriptor
        let pipe = Async::new(pipe_file)?;
        Ok(Self { pipe })
    }

    async fn write<'a>(&'a mut self, buf: &'a [u8]) -> ZResult<usize> {
        let result = self
            .pipe
            .write_with_mut(|pipe| match pipe.write(buf) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => Ok(val),
                Err(e) => Err(e),
            })
            .await?;
        ZResult::Ok(result)
    }

    async fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> ZResult<()> {
        let mut r: usize = 0;
        self.pipe
            .write_with_mut(|pipe| match pipe.write(&buf[r..]) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => {
                    r += val;
                    if r == buf.len() {
                        return Ok(());
                    }
                    Err(async_std::io::ErrorKind::WouldBlock.into())
                }
                Err(e) => Err(e),
            })
            .await?;
        ZResult::Ok(())
    }

    fn open_unique_pipe_for_write(path: &str) -> ZResult<File> {
        let write = open_write(path)?;
        // the file must be already locked at the other side...
        #[cfg(not(target_os = "macos"))]
        if write.try_lock(FileLockMode::Exclusive).is_ok() {
            let _ = write.unlock();
            bail!("no listener...")
        }
        Ok(write)
    }
}

async fn handle_incoming_connections(
    endpoint: &EndPoint,
    manager: &Arc<NewLinkChannelSender>,
    request_channel: &mut PipeR,
    path_downlink: &str,
    path_uplink: &str,
    access_mode: u32,
) -> ZResult<()> {
    // read invitation from the request channel
    let suffix = Invitation::receive(request_channel).await?;

    // gererate uplink and downlink names
    let (dedicated_downlink_path, dedicated_uplink_path) =
        get_dedicated_pipe_names(path_downlink, path_uplink, suffix);

    // create dedicated downlink and uplink
    let mut dedicated_downlink = PipeW::new(&dedicated_downlink_path).await?;
    let mut dedicated_uplink = PipeR::new(&dedicated_uplink_path, access_mode).await?;

    // confirm over the dedicated chanel
    Invitation::confirm(suffix, &mut dedicated_downlink).await?;

    // got confirmation over the dedicated chanel
    Invitation::expect(suffix, &mut dedicated_uplink).await?;

    // create Locators
    let local = Locator::new(
        endpoint.protocol(),
        dedicated_uplink_path,
        endpoint.metadata(),
    )?;
    let remote = Locator::new(
        endpoint.protocol(),
        dedicated_downlink_path,
        endpoint.metadata(),
    )?;

    // send newly established link to manager
    manager
        .send_async(LinkUnicast(Arc::new(UnicastPipe {
            r: UnsafeCell::new(dedicated_uplink),
            w: UnsafeCell::new(dedicated_downlink),
            local,
            remote,
        })))
        .await?;

    ZResult::Ok(())
}

struct UnicastPipeListener {
    listening_task_handle: JoinHandle<ZResult<()>>,
    uplink_locator: Locator,
}
impl UnicastPipeListener {
    async fn listen(endpoint: EndPoint, manager: Arc<NewLinkChannelSender>) -> ZResult<Self> {
        let (path_uplink, path_downlink, access_mode) = parse_pipe_endpoint(&endpoint);
        let local = Locator::new(
            endpoint.protocol(),
            path_uplink.as_str(),
            endpoint.metadata(),
        )?;

        // create request channel
        let mut request_channel = PipeR::new(&path_uplink, access_mode).await?;

        // create listening task
        let listening_task_handle = async_std::task::spawn(async move {
            loop {
                let _ = handle_incoming_connections(
                    &endpoint,
                    &manager,
                    &mut request_channel,
                    &path_downlink,
                    &path_uplink,
                    access_mode,
                )
                .await;
            }
        });

        Ok(Self {
            listening_task_handle,
            uplink_locator: local,
        })
    }

    async fn stop_listening(self) {
        self.listening_task_handle.cancel().await;
    }
}

fn get_dedicated_pipe_names(
    path_downlink: &str,
    path_uplink: &str,
    suffix: u32,
) -> (String, String) {
    let suffix_str = suffix.to_string();
    let path_uplink = path_uplink.to_string() + &suffix_str;
    let path_downlink = path_downlink.to_string() + &suffix_str;
    (path_downlink, path_uplink)
}

async fn create_pipe(
    path_uplink: &str,
    path_downlink: &str,
    access_mode: u32,
) -> ZResult<(PipeR, u32, String, String)> {
    // generate random suffix
    let suffix: u32 = rand::thread_rng().gen();

    // gererate uplink and downlink names
    let (path_downlink, path_uplink) = get_dedicated_pipe_names(path_downlink, path_uplink, suffix);

    // try create uplink and downlink pipes to ensure that the selected suffix is available
    let downlink = PipeR::new(&path_downlink, access_mode).await?;
    let _uplink = PipeR::new(&path_uplink, access_mode).await?; // uplink would be dropped, that is OK!

    Ok((downlink, suffix, path_downlink, path_uplink))
}

async fn dedicate_pipe(
    path_uplink: &str,
    path_downlink: &str,
    access_mode: u32,
) -> ZResult<(PipeR, u32, String, String)> {
    for _ in 0..LINUX_PIPE_DEDICATE_TRIES {
        match create_pipe(path_uplink, path_downlink, access_mode).await {
            Err(_) => {}
            val => {
                return val;
            }
        }
    }
    bail!("Unabe to dedicate pipe!")
}

struct UnicastPipeClient;
impl UnicastPipeClient {
    async fn connect_to(endpoint: EndPoint) -> ZResult<UnicastPipe> {
        let (path_uplink, path_downlink, access_mode) = parse_pipe_endpoint(&endpoint);

        // open the request channel
        // this channel would be used to invite listener to the dedicated channel
        // listener owns the request channel, so failure of this call means that there is nobody listening on the provided endpoint
        let mut request_channel = PipeW::new(&path_uplink).await?;

        // create dedicated channel prerequisities. The creation code also ensures that nobody else would use the same channel concurrently
        let (
            mut dedicated_downlink,
            dedicated_suffix,
            dedicated_donlink_path,
            dedicated_uplink_path,
        ) = dedicate_pipe(&path_uplink, &path_downlink, access_mode).await?;

        // invite the listener to our dedicated channel over the requet channel
        Invitation::send(dedicated_suffix, &mut request_channel).await?;

        // read responce that should be sent over the dedicated channel, confirming that everything is OK
        // on the listener's side and it is already working with the dedicated channel
        Invitation::expect(dedicated_suffix, &mut dedicated_downlink).await?;

        // open dedicated uplink
        let mut dedicated_uplink = PipeW::new(&dedicated_uplink_path).await?;

        // final confirmation over the dedicated uplink
        Invitation::confirm(dedicated_suffix, &mut dedicated_uplink).await?;

        // create Locators
        let local = Locator::new(
            endpoint.protocol(),
            dedicated_donlink_path,
            endpoint.metadata(),
        )?;
        let remote = Locator::new(
            endpoint.protocol(),
            dedicated_uplink_path,
            endpoint.metadata(),
        )?;

        Ok(UnicastPipe {
            r: UnsafeCell::new(dedicated_downlink),
            w: UnsafeCell::new(dedicated_uplink),
            local,
            remote,
        })
    }
}

struct UnicastPipe {
    // The underlying pipes wrapped into async_io
    // SAFETY: Async requires &mut for read and write operations. This means
    //         that concurrent reads and writes are not possible. To achieve that,
    //         we use an UnsafeCell for interior mutability. Using an UnsafeCell
    //         is safe in our case since the transmission and reception logic
    //         already ensures that no concurrent reads or writes can happen on
    //         the same stream: there is only one task at the time that writes on
    //         the stream and only one task at the time that reads from the stream.
    r: UnsafeCell<PipeR>,
    w: UnsafeCell<PipeW>,
    local: Locator,
    remote: Locator,
}

impl UnicastPipe {
    // SAFETY: It is safe to suppress Clippy warning since no concurrent access will ever happen.
    // The write and read pipes are independent and support full-duplex operation,
    // and single-direction operations are aligned at the transport side and will never access link concurrently
    #[allow(clippy::mut_from_ref)]
    fn get_r_mut(&self) -> &mut PipeR {
        unsafe { &mut *self.r.get() }
    }

    #[allow(clippy::mut_from_ref)]
    fn get_w_mut(&self) -> &mut PipeW {
        unsafe { &mut *self.w.get() }
    }
}
// Promise that proper synchronization exists *around accesses*.
unsafe impl Sync for UnicastPipe {}

impl Drop for UnicastPipe {
    fn drop(&mut self) {}
}

#[async_trait]
impl LinkUnicastTrait for UnicastPipe {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing Unix Pipe link: {}", self);
        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        self.get_w_mut().write(buffer).await
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        self.get_w_mut().write_all(buffer).await
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        self.get_r_mut().read(buffer).await
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        self.get_r_mut().read_exact(buffer).await
    }

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.local
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.remote
    }

    #[inline(always)]
    fn get_mtu(&self) -> u16 {
        LINUX_PIPE_MAX_MTU
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        true
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }
}

impl fmt::Display for UnicastPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.local, self.remote)?;
        Ok(())
    }
}

impl fmt::Debug for UnicastPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnicastPipe")
            .field("src", &self.local)
            .field("dst", &self.remote)
            .finish()
    }
}

pub struct LinkManagerUnicastPipe {
    manager: Arc<NewLinkChannelSender>,
    listeners: async_std::sync::RwLock<HashMap<EndPoint, UnicastPipeListener>>,
}

impl LinkManagerUnicastPipe {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager: Arc::new(manager),
            listeners: async_std::sync::RwLock::new(HashMap::new()),
        }
    }
}
impl ConstructibleLinkManagerUnicast<()> for LinkManagerUnicastPipe {
    fn new(new_link_sender: NewLinkChannelSender, _: ()) -> ZResult<Self> {
        Ok(Self::new(new_link_sender))
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastPipe {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let pipe = UnicastPipeClient::connect_to(endpoint).await?;
        Ok(LinkUnicast(Arc::new(pipe)))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let listener = UnicastPipeListener::listen(endpoint.clone(), self.manager.clone()).await?;
        let locator = listener.uplink_locator.clone();
        zasyncwrite!(self.listeners).insert(endpoint, listener);
        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let removed = zasyncwrite!(self.listeners).remove(endpoint);
        match removed {
            Some(val) => {
                val.stop_listening().await;
                Ok(())
            }
            None => bail!("No listener found for endpoint {}", endpoint),
        }
    }

    fn get_listeners(&self) -> Vec<EndPoint> {
        async_std::task::block_on(async { zasyncread!(self.listeners) })
            .keys()
            .cloned()
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        async_std::task::block_on(async { zasyncread!(self.listeners) })
            .values()
            .map(|v| v.uplink_locator.clone())
            .collect()
    }
}

fn parse_pipe_endpoint(endpoint: &EndPoint) -> (String, String, u32) {
    let address = endpoint.address();
    let path = address.as_str();
    let path_uplink = path.to_string() + "_uplink";
    let path_downlink = path.to_string() + "_downlink";
    let access_mode = endpoint
        .config()
        .get(config::FILE_ACCESS_MASK)
        .map_or(*FILE_ACCESS_MASK, |val| {
            val.parse().unwrap_or(*FILE_ACCESS_MASK)
        });
    (path_uplink, path_downlink, access_mode)
}
