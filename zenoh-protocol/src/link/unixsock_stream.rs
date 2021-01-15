//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::{Link, LinkManagerTrait, LinkTrait, Locator};
use crate::session::SessionManager;
use async_std::channel::{bounded, Receiver, Sender};
use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::path::{Path, PathBuf};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, RwLock};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::fs::remove_file;
use std::net::Shutdown;
use std::time::Duration;
use uuid::Uuid;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasyncread, zasyncwrite, zerror, zerror2};

// Default MTU (UnixSock-Stream PDU) in bytes.
// NOTE: Since UnixSock-Stream is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the UNIXSOCKSTREAM MTU is constrained to
//       2^16 + 1 bytes (i.e., 65537).
const UNIXSOCKSTREAM_MAX_MTU: usize = 65_537;

zconfigurable! {
    // Default MTU (UNIXSOCKSTREAM PDU) in bytes.
    static ref UNIXSOCKSTREAM_DEFAULT_MTU: usize = UNIXSOCKSTREAM_MAX_MTU;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UNIXSOCKSTREAM_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[allow(unreachable_patterns)]
fn get_unix_path(locator: &Locator) -> ZResult<&PathBuf> {
    match locator {
        Locator::UnixSockStream(path) => Ok(path),
        _ => {
            let e = format!("Not a UnixSock-Stream locator: {:?}", locator);
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
fn get_unix_path_as_string(locator: &Locator) -> String {
    match locator {
        Locator::UnixSockStream(path) => match path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => {
                let e = format!("Not a UnixSock-Stream locator: {:?}", locator);
                log::debug!("{}", e);
                "None".to_string()
            }
        },
        _ => {
            let e = format!("Not a UnixSock-Stream locator: {:?}", locator);
            log::debug!("{}", e);
            "None".to_string()
        }
    }
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct UnixSockStream {
    // The underlying socket as returned from the async-std library
    socket: UnixStream,
    // The Unix domain socket source path
    src_path: String,
    // The Unix domain socker destination path (random UUIDv4)
    dst_path: String,
}

impl UnixSockStream {
    fn new(socket: UnixStream, src_path: String, dst_path: String) -> UnixSockStream {
        UnixSockStream {
            socket,
            src_path,
            dst_path,
        }
    }
}

#[async_trait]
impl LinkTrait for UnixSockStream {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UnixSock-Stream link: {}", self);
        // Close the underlying UnixSock-Stream socket
        let res = self.socket.shutdown(Shutdown::Both);
        log::trace!("UnixSock-Stream link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IOError {
                descr: format!("{}", e),
            })
        })
    }

    #[inline]
    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match (&self.socket).write(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Transmission error on UnixSock-Stream link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        match (&self.socket).write_all(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Transmission error on UnixSock-Stream link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match (&self.socket).read(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Reception error on UnixSock-Stream link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        match (&self.socket).read_exact(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Reception error on UnixSock-Stream link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    fn get_src(&self) -> Locator {
        Locator::UnixSockStream(PathBuf::from(self.src_path.clone()))
    }

    #[inline]
    fn get_dst(&self) -> Locator {
        Locator::UnixSockStream(PathBuf::from(self.dst_path.clone()))
    }

    #[inline]
    fn get_mtu(&self) -> usize {
        *UNIXSOCKSTREAM_DEFAULT_MTU
    }

    #[inline]
    fn is_reliable(&self) -> bool {
        true
    }

    #[inline]
    fn is_streamed(&self) -> bool {
        true
    }
}

impl Drop for UnixSockStream {
    fn drop(&mut self) {
        // Close the underlying UnixSock-Stream socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for UnixSockStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_path, self.dst_path)?;
        Ok(())
    }
}

impl fmt::Debug for UnixSockStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixSockStream")
            .field("src", &self.src_path)
            .field("dst", &self.dst_path)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnixSockStream {
    socket: Arc<UnixListener>,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerUnixSockStream {
    fn new(socket: Arc<UnixListener>) -> ListenerUnixSockStream {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerUnixSockStream {
            socket,
            sender,
            receiver,
            barrier,
        }
    }
}

pub struct LinkManagerUnixSockStream {
    manager: SessionManager,
    listener: Arc<RwLock<HashMap<String, Arc<ListenerUnixSockStream>>>>,
}

impl LinkManagerUnixSockStream {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerUnixSockStream {
    async fn new_link(&self, locator: &Locator) -> ZResult<Link> {
        let path = get_unix_path(locator)?;

        // Create the UnixSock-Stream connection
        let stream = match UnixStream::connect(path).await {
            Ok(stream) => stream,
            Err(e) => {
                let e = format!(
                    "Can not create a new UnixSock-Stream link bound to {:?}: {}",
                    path, e
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }
        };

        let src_addr = match stream.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!(
                    "Can not create a new UnixSock-Stream link bound to {:?}: {}",
                    path, e
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        // We do need the dst_addr value, we just need to check that is valid
        if let Err(e) = stream.peer_addr() {
            let e = format!(
                "Can not create a new UnixSock-Stream link bound to {:?}: {}",
                path, e
            );
            log::warn!("{}", e);
            return zerror!(ZErrorKind::InvalidLink { descr: e });
        }

        let local_path = match src_addr.as_pathname() {
            Some(path) => PathBuf::from(path),
            None => {
                let e = format!(
                    "Can not create a new UnixSock-Stream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                PathBuf::from(format!("{}", Uuid::new_v4()))
            }
        };

        let local_path_str = match local_path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => {
                let e = format!(
                    "Can not create a new UnixSock-Stream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let remote_path_str = match path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => {
                let e = format!(
                    "Can not create a new UnixSock-Stream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let link = Arc::new(UnixSockStream::new(stream, local_path_str, remote_path_str));

        Ok(Link::new(link))
    }

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let path = get_unix_path_as_string(locator);

        // Bind the Unix socket
        let socket = match UnixListener::bind(&path).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                let e = format!(
                    "Can not create a new UnixSock-Stream listener on {}: {}",
                    path, e
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!(
                    "Can not create a new UnixSock-Stream listener on {}: {}",
                    path, e
                );
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_path = match local_addr.as_pathname() {
            Some(path) => PathBuf::from(path),
            None => {
                let e = format!("Can not create a new UnixSock-Stream listener on {}", path);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_path_str = match local_path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => "None".to_string(),
        };

        let listener = Arc::new(ListenerUnixSockStream::new(socket));
        // Update the list of active listeners on the manager
        zasyncwrite!(self.listener).insert(local_path_str.clone(), listener.clone());

        // Spawn the accept loop for the listener
        let c_listeners = self.listener.clone();
        let c_path = local_path_str.clone();
        let c_manager = self.manager.clone();
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_task(listener, c_manager).await;
            // Delete the listener from the manager
            zasyncwrite!(c_listeners).remove(&c_path);
        });

        Ok(Locator::UnixSockStream(local_path))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let path = get_unix_path_as_string(locator);

        // Stop the listener
        match zasyncwrite!(self.listener).remove(&path) {
            Some(listener) => {
                // Send the stop signal
                let res = listener.sender.send(()).await;
                if res.is_ok() {
                    // Wait for the accept loop to be stopped
                    listener.barrier.wait().await;
                }
                // Remove the Unix Domain Socket file
                let res = remove_file(path);
                log::trace!("UnixSock-Stream Domain Socket removal result: {:?}", res);
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the UnixSock-Stream listener because it has not been found: {}",
                    path
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn get_listeners(&self) -> Vec<Locator> {
        zasyncread!(self.listener)
            .keys()
            .map(|x| Locator::UnixSockStream(PathBuf::from(x)))
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];
        for addr in zasyncread!(self.listener).keys() {
            let path = Path::new(&addr);
            if !path.exists().await {
                log::error!("Unable to get local addresses : {}", addr);
            } else {
                locators.push(PathBuf::from(addr));
            }
        }
        locators.into_iter().map(Locator::UnixSockStream).collect()
    }
}

async fn accept_task(listener: Arc<ListenerUnixSockStream>, manager: SessionManager) {
    // The accept future
    let accept_loop = async {
        log::trace!(
            "Ready to accept UnixSock-Stream connections on: {:?}",
            listener.socket.local_addr()
        );
        loop {
            // Wait for incoming connections
            let stream = match listener.socket.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    log::warn!(
                        "{}. Hint: you might want to increase the system open file limit",
                        e
                    );
                    // Throttle the accept loop upon an error
                    // NOTE: This might be due to various factors. However, the most common case is that
                    //       the process has reached the maximum number of open files in the system. On
                    //       Linux systems this limit can be changed by using the "ulimit" command line
                    //       tool. In case of systemd-based systems, this can be changed by using the
                    //       "sysctl" command line tool.
                    task::sleep(Duration::from_micros(*UNIXSOCKSTREAM_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }
            };
            // Get the source UnixSock-Stream addresses
            let src_addr = match stream.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept UnixSock-Stream connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };
            // We do need the dst_addr value, we just need to check that is valid
            if let Err(e) = stream.peer_addr() {
                let e = format!("Can not accept UnixSock-Stream connection: {}", e);
                log::warn!("{}", e);
                continue;
            }

            let local_path = match src_addr.as_pathname() {
                Some(path) => PathBuf::from(path),
                None => {
                    let e = format!(
                        "Can not create a new UnixSock-Stream link bound to {:?}",
                        src_addr
                    );
                    log::warn!("{}", e);
                    continue;
                }
            };
            let src_path = match local_path.to_str() {
                Some(path_str) => path_str.to_string(),
                None => {
                    let e = format!(
                        "Can not create a new UnixSock-Stream link bound to {:?}",
                        src_addr
                    );
                    log::warn!("{}", e);
                    continue;
                }
            };

            let dst_path = format!("{}", Uuid::new_v4());

            log::debug!("Accepted UnixSock-Stream connection on: {:?}", src_addr,);

            // Create the new link object
            let link = Arc::new(UnixSockStream::new(stream, src_path, dst_path));

            // Communicate the new link to the initial session manager
            manager.handle_new_link(Link::new(link)).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
