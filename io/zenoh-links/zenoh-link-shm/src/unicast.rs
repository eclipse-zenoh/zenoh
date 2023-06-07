use crate::config;

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
use advisory_lock::{AdvisoryFileLock, FileLockMode};
use async_io::Async;
use async_std::io::WriteExt;
use async_std::task::JoinHandle;
use async_std::fs::remove_file;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::Arc;

use unix_named_pipe::{create, open_read, open_write};

use zenoh_link_commons::{
    ConstructibleLinkManagerUnicast, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, ZResult};

const LINUX_PIPE_MAX_MTU: u16 = 65_535;

static PIPE_INVITATION: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];

struct PipeR {
    pipe: Async<File>,
}
impl PipeR {
    async fn new(path: &str, access_mode: u32) -> ZResult<Self> {
        // create, open and lock named pipe
        let pipe_file = create_and_open_unique_pipe_for_read(path, access_mode).await?;
        // create async_io wrapper for pipe's file descriptor
        let pipe = Async::new(pipe_file)?;
        Ok(Self { pipe })
    }

    async fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> ZResult<usize> {
        let result = self.pipe
            .read_with_mut(|pipe| match pipe.read(&mut buf[..]) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => {
                    Ok(val)
                }
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
}

struct PipeW {
    pipe: Async<File>,
}
impl PipeW {
    async fn new(path: &str) -> ZResult<Self> {
        // create, open and lock named pipe
        let pipe_file = open_unique_pipe_for_write(path)?;
        // create async_io wrapper for pipe's file descriptor
        let pipe = Async::new(pipe_file)?;
        Ok(Self { pipe })
    }

    async fn write<'a>(&'a mut self, buf: &'a [u8]) -> ZResult<usize> {
        let result = self.pipe
            .write_with_mut(|pipe| match pipe.write(buf) {
                Ok(0) => Err(async_std::io::ErrorKind::WouldBlock.into()),
                Ok(val) => {
                    Ok(val)
                } 
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
}

struct UnicastPipeListener {
    listening_task_handle: JoinHandle<ZResult<()>>,
    uplink_locator: Locator,
}
impl UnicastPipeListener {
    async fn listen(endpoint: EndPoint, manager: Arc<NewLinkChannelSender>) -> ZResult<Self> {
        let (path_uplink, path_downlink, access_mode) = parse_pipe_endpoint(&endpoint);

        // create Locators
        let local = Locator::new(
            endpoint.protocol(),
            path_uplink.as_str(),
            endpoint.metadata(),
        )?;
        let c_local = local.clone();
        let remote = Locator::new(
            endpoint.protocol(),
            path_downlink.as_str(),
            endpoint.metadata(),
        )?;

        // create uplink
        let mut uplink = PipeR::new(&path_uplink, access_mode).await?;

        // create listening task
        let listening_task_handle = async_std::task::spawn(async move {
            // read invitation from uplink pipe and check it's correctness
            let mut invitation = [0; 4];
            uplink.read_exact(&mut invitation).await?;
            if invitation != PIPE_INVITATION {
                bail!("Unexpected invitation received during pipe handshake!")
            }

            // create downlink
            let mut downlink = PipeW::new(&path_downlink).await?;
            // echo invitation to other party
            downlink.write_all(&invitation).await?;

            // send newly established link to manager
            manager
                .send_async(LinkUnicast(Arc::new(UnicastPipe {
                    r: async_std::sync::RwLock::new(uplink),
                    w: async_std::sync::RwLock::new(downlink),
                    local,
                    remote,
                })))
                .await?;

            ZResult::Ok(())
        });

        Ok(Self {
            listening_task_handle,
            uplink_locator: c_local,
        })
    }

    async fn stop_listening(self) {
        self.listening_task_handle.cancel().await;
    }
}

struct UnicastPipeClient;
impl UnicastPipeClient {
    async fn connect_to(endpoint: EndPoint) -> ZResult<UnicastPipe> {
        let (path_uplink, path_downlink, access_mode) = parse_pipe_endpoint(&endpoint);

        // create Locators
        let local = Locator::new(
            endpoint.protocol(),
            path_downlink.as_str(),
            endpoint.metadata(),
        )?;
        let remote = Locator::new(
            endpoint.protocol(),
            path_uplink.as_str(),
            endpoint.metadata(),
        )?;

        // create uplink
        let mut uplink = PipeW::new(&path_uplink).await?;
        // create downlink
        let mut downlink = PipeR::new(&path_downlink, access_mode).await?;

        // write invitation
        uplink.write_all(PIPE_INVITATION).await?;

        // read responce
        let mut responce: [u8; 4] = [0, 0, 0, 0];
        downlink.read_exact(&mut responce).await?;
        // check responce
        if responce != PIPE_INVITATION {
            bail!("Unexpected responce received during pipe handshake!")
        }

        Ok(UnicastPipe {
            r: async_std::sync::RwLock::new(downlink),
            w: async_std::sync::RwLock::new(uplink),
            local,
            remote,
        })
    }
}

struct UnicastPipe {
    r: async_std::sync::RwLock<PipeR>,
    w: async_std::sync::RwLock<PipeW>,
    local: Locator,
    remote: Locator,
}
impl Drop for UnicastPipe {
    fn drop(&mut self) {}
}
#[async_trait]
impl LinkUnicastTrait for UnicastPipe {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing SHM Pipe link: {}", self);
        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        //todo: I completely don't like the idea of locking reader\writer each time, but LinkUnicastTrait requires self.r and self.w to be Sync
        self.w
            .write()
            .await
            .write(buffer)
            .await
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        //todo: I completely don't like the idea of locking reader\writer each time, but LinkUnicastTrait requires self.r and self.w to be Sync
        self.w
            .write()
            .await
            .write_all(buffer)
            .await
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        //todo: I completely don't like the idea of locking reader\writer each time, but LinkUnicastTrait requires self.r and self.w to be Sync
        self.r
            .write()
            .await
            .read(buffer)
            .await
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        //todo: I completely don't like the idea of locking reader\writer each time, but LinkUnicastTrait requires self.r and self.w to be Sync
        self.r
            .write()
            .await
            .read_exact(buffer)
            .await
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
        f.debug_struct("Shm")
            .field("src", &self.local)
            .field("dst", &self.remote)
            .finish()
    }
}

pub struct LinkManagerUnicastPipe {
    manager: Arc<NewLinkChannelSender>,
    listeners: std::sync::RwLock<HashMap<EndPoint, UnicastPipeListener>>,
}

impl LinkManagerUnicastPipe {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager: Arc::new(manager),
            listeners: std::sync::RwLock::new(HashMap::new()),
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
        self.listeners.write().unwrap().insert(endpoint, listener);
        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let removed = self.listeners.write().unwrap().remove(endpoint);
        match removed {
            Some(val) => {
                val.stop_listening().await;
                Ok(())
            }
            None => bail!("No listener found for endpoint {}", endpoint),
        }
    }

    fn get_listeners(&self) -> Vec<EndPoint> {
        self.listeners.read().unwrap().keys().cloned().collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        self.listeners
            .read()
            .unwrap()
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
        .get(config::SHM_ACCESS_MASK)
        .map_or(*crate::SHM_ACCESS_MASK, |val| {
            val.parse().unwrap_or(*crate::SHM_ACCESS_MASK)
        });
    (path_uplink, path_downlink, access_mode)
}

async fn create_and_open_unique_pipe_for_read(path_r: &str, access_mode: u32) -> ZResult<File> {
    let r_was_created = create(path_r, Some(access_mode));
    let open_result = open_unique_pipe_for_read(path_r);
    match (open_result.is_ok(), r_was_created.is_ok()) {
        (false, true) => {
            // clean-up in case of failure
            let _ = remove_file(path_r).await;
        }
        (true, false) => {
            // drop all the data from the pipe in case if it already exists
            let mut buf: [u8; 1] = [0; 1];
            while let Ok(val) = open_result.as_ref().unwrap().read(&mut buf) {
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

    read.try_lock(FileLockMode::Exclusive)?;
    Ok(read)
}

fn open_unique_pipe_for_write(path: &str) -> ZResult<File> {
    let write = open_write(path)?;
    // the file must be already locked at the other side...
    match write.try_lock(FileLockMode::Exclusive) {
        Ok(_) => {
            let _ = write.unlock();
            bail!("no listener...")
        }
        Err(_) => Ok(write),
    }
}
