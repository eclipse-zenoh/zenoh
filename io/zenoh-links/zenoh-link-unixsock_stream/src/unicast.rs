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
use super::UNIXSOCKSTREAM_ACCEPT_THROTTLE_TIME;
use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::path::PathBuf;
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use futures::io::AsyncReadExt;
use futures::io::AsyncWriteExt;
use std::collections::HashMap;
use std::fmt;
use std::fs::remove_file;
use std::net::Shutdown;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use uuid::Uuid;
use zenoh_core::{zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::Signal;

use super::{get_unix_path_as_string, UNIXSOCKSTREAM_DEFAULT_MTU, UNIXSOCKSTREAM_LOCATOR_PREFIX};

pub struct LinkUnicastUnixSocketStream {
    // The underlying socket as returned from the async-std library
    socket: UnixStream,
    // The Unix domain socket source path
    src_locator: Locator,
    // The Unix domain socker destination path (random UUIDv4)
    dst_locator: Locator,
}

impl LinkUnicastUnixSocketStream {
    fn new(socket: UnixStream, src_path: &str, dst_path: &str) -> LinkUnicastUnixSocketStream {
        LinkUnicastUnixSocketStream {
            socket,
            src_locator: Locator::new(UNIXSOCKSTREAM_LOCATOR_PREFIX, src_path, "").unwrap(),
            dst_locator: Locator::new(UNIXSOCKSTREAM_LOCATOR_PREFIX, dst_path, "").unwrap(),
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastUnixSocketStream {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UnixSocketStream link: {}", self);
        // Close the underlying UnixSocketStream socket
        let res = self.socket.shutdown(Shutdown::Both);
        log::trace!("UnixSocketStream link shutdown {}: {:?}", self, res);
        res.map_err(|e| zerror!(e).into())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        (&self.socket).write(buffer).await.map_err(|e| {
            let e = zerror!("Write error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        (&self.socket).write_all(buffer).await.map_err(|e| {
            let e = zerror!("Write error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        (&self.socket).read(buffer).await.map_err(|e| {
            let e = zerror!("Read error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        (&self.socket).read_exact(buffer).await.map_err(|e| {
            let e = zerror!("Read error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
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
        *UNIXSOCKSTREAM_DEFAULT_MTU
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

impl Drop for LinkUnicastUnixSocketStream {
    fn drop(&mut self) {
        // Close the underlying UnixSocketStream socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for LinkUnicastUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", &self.src_locator, &self.dst_locator)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixSocketStream")
            .field("src", &self.src_locator)
            .field("dst", &self.dst_locator)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnixSocketStream {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
    lock_fd: RawFd,
}

impl ListenerUnixSocketStream {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
        lock_fd: RawFd,
    ) -> ListenerUnixSocketStream {
        ListenerUnixSocketStream {
            endpoint,
            active,
            signal,
            handle,
            lock_fd,
        }
    }
}

pub struct LinkManagerUnicastUnixSocketStream {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<String, ListenerUnixSocketStream>>>,
}

impl LinkManagerUnicastUnixSocketStream {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastUnixSocketStream {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let path = get_unix_path_as_string(endpoint.address());

        // Create the UnixSocketStream connection
        let stream = UnixStream::connect(&path).await.map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        let src_addr = stream.local_addr().map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        // We do need the dst_addr value, we just need to check that is valid
        let _dst_addr = stream.peer_addr().map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        let local_path = match src_addr.as_pathname() {
            Some(path) => PathBuf::from(path),
            None => {
                let e = format!("Can not create a new UnixSocketStream link bound to {path:?}");
                log::warn!("{}", e);
                PathBuf::from(format!("{}", Uuid::new_v4()))
            }
        };

        let local_path_str = local_path.to_str().ok_or_else(|| {
            let e = zerror!(
                "Can not create a new UnixSocketStream link bound to {:?}",
                path
            );
            log::warn!("{}", e);
            e
        })?;

        let remote_path_str = path.as_str();

        let link = Arc::new(LinkUnicastUnixSocketStream::new(
            stream,
            local_path_str,
            remote_path_str,
        ));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let path = get_unix_path_as_string(endpoint.address());

        // Because of the lack of SO_REUSEADDR we have to check if the
        // file is still there and if it is not used by another process.
        // In order to do so we use a separate lock file.
        // If the lock CAN NOT be acquired means that another process is
        // holding the lock NOW, therefore we cannot use the socket.
        // Kernel guarantees that the lock is release if the owner exists
        // or crashes.

        // If the lock CAN be acquired means no one is using the socket.
        // Therefore we can unlink the socket file and create the new one with
        // bind(2)

        // We generate the path for the lock file, by adding .lock
        // to the socket file
        let lock_file_path = format!("{path}.lock");

        // We try to open the lock file, with O_RDONLY | O_CREAT
        // and mode S_IRUSR | S_IWUSR, user read-write permissions
        let mut open_flags = nix::fcntl::OFlag::empty();

        open_flags.insert(nix::fcntl::OFlag::O_CREAT);
        open_flags.insert(nix::fcntl::OFlag::O_RDONLY);

        let mut open_mode = nix::sys::stat::Mode::empty();
        open_mode.insert(nix::sys::stat::Mode::S_IRUSR);
        open_mode.insert(nix::sys::stat::Mode::S_IWUSR);

        let lock_fd = nix::fcntl::open(
            std::path::Path::new(&lock_file_path),
            open_flags,
            open_mode,
        ).map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream listener on {} - Unable to open lock file: {}",
                path, e
            );
            log::warn!("{}", e);
            e
        })?;

        // We try to acquire the lock
        nix::fcntl::flock(lock_fd, nix::fcntl::FlockArg::LockExclusiveNonblock).map_err(|e| {
            let _ = nix::unistd::close(lock_fd);
            let e = zerror!(
                "Can not create a new UnixSocketStream listener on {} - Unable to acquire look: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        //Lock is acquired we can remove the socket file
        // If the file does not exist this would return an error.
        // We are not interested if the file was not existing.
        let _ = remove_file(path.clone());

        // Bind the Unix socket
        let socket = UnixListener::bind(&path).await.map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream listener on {}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            let e = zerror!(
                "Can not create a new UnixSocketStream listener on {}: {}",
                path,
                e
            );
            log::warn!("{}", e);
            e
        })?;

        let local_path = PathBuf::from(local_addr.as_pathname().ok_or_else(|| {
            let e = zerror!("Can not create a new UnixSocketStream listener on {}", path);
            log::warn!("{}", e);
            e
        })?);

        let local_path_str = local_path.to_str().ok_or_else(|| {
            let e = zerror!("Can not create a new UnixSocketStream listener on {}", path);
            log::warn!("{}", e);
            e
        })?;

        // Update the endpoint with the acutal local path
        endpoint = EndPoint::new(
            endpoint.protocol(),
            local_path_str,
            endpoint.metadata(),
            endpoint.config(),
        )?;

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();
        let mut listeners = zwrite!(self.listeners);

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_path = local_path_str.to_owned();
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(socket, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_path);
            res
        });

        let locator = endpoint.to_locator();
        let listener = ListenerUnixSocketStream::new(endpoint, active, signal, handle, lock_fd);
        listeners.insert(local_path_str.to_owned(), listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let path = get_unix_path_as_string(endpoint.address());

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&path).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the UnixSocketStream listener because it has not been found: {}",
                path
            );
            log::trace!("{}", e);
            e
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        let res = listener.handle.await;

        //Release the lock
        let _ = nix::fcntl::flock(listener.lock_fd, nix::fcntl::FlockArg::UnlockNonblock);
        let _ = nix::unistd::close(listener.lock_fd);
        let _ = remove_file(path.clone());

        // Remove the Unix Domain Socket file
        let lock_file_path = format!("{path}.lock");
        let tmp = remove_file(lock_file_path);
        log::trace!("UnixSocketStream Domain Socket removal result: {:?}", tmp);
        res
    }

    fn get_listeners(&self) -> Vec<EndPoint> {
        zread!(self.listeners)
            .values()
            .map(|x| x.endpoint.clone())
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        zread!(self.listeners)
            .values()
            .map(|x| x.endpoint.to_locator())
            .collect()
    }
}

async fn accept_task(
    socket: UnixListener,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept(UnixStream),
        Stop,
    }

    async fn accept(socket: &UnixListener) -> ZResult<Action> {
        let (stream, _) = socket.accept().await.map_err(|e| zerror!(e))?;
        Ok(Action::Accept(stream))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        zerror!("Can not accept UnixSocketStream connections: {}", e);
        log::warn!("{}", e);
        e
    })?;

    let local_path = PathBuf::from(src_addr.as_pathname().ok_or_else(|| {
        let e = zerror!(
            "Can not create a new UnixSocketStream link bound to {:?}",
            src_addr
        );
        log::warn!("{}", e);
        e
    })?);

    let src_path = local_path.to_str().ok_or_else(|| {
        let e = zerror!(
            "Can not create a new UnixSocketStream link bound to {:?}",
            src_addr
        );
        log::warn!("{}", e);
        e
    })?;

    // The accept future
    log::trace!(
        "Ready to accept UnixSocketStream connections on: {}",
        src_path
    );
    while active.load(Ordering::Acquire) {
        // Wait for incoming connections
        let stream = match accept(&socket).race(stop(signal.clone())).await {
            Ok(action) => match action {
                Action::Accept(stream) => stream,
                Action::Stop => break,
            },
            Err(e) => {
                log::warn!("{}. Hint: increase the system open file limit.", e);
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

        let dst_path = format!("{}", Uuid::new_v4());

        log::debug!("Accepted UnixSocketStream connection on: {:?}", src_addr,);

        // Create the new link object
        let link = Arc::new(LinkUnicastUnixSocketStream::new(
            stream, src_path, &dst_path,
        ));

        // Communicate the new link to the initial transport manager
        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}
