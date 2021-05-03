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
use super::session::SessionManager;
use super::{Link, LinkManagerTrait, Locator, LocatorProperty};
use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::path::{Path, PathBuf};
use async_std::prelude::*;
use async_std::sync::{Arc, RwLock};
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::fs::remove_file;
use std::net::Shutdown;
use std::os::unix::io::RawFd;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Signal;
use zenoh_util::{zasyncread, zasyncwrite, zerror, zerror2};

// Default MTU (UnixSocketStream PDU) in bytes.
// NOTE: Since UnixSocketStream is a byte-stream oriented transport, theoretically it has
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
fn get_unix_path(locator: &Locator) -> ZResult<PathBuf> {
    match locator {
        Locator::UnixSocketStream(path) => Ok(path.0.clone()),
        _ => {
            let e = format!("Not a UnixSocketStream locator: {:?}", locator);
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
fn get_unix_path_as_string(locator: &Locator) -> String {
    match locator {
        Locator::UnixSocketStream(path) => match path.0.to_str() {
            Some(path_str) => path_str.to_string(),
            None => {
                let e = format!("Not a UnixSocketStream locator: {:?}", locator);
                log::debug!("{}", e);
                "None".to_string()
            }
        },
        _ => {
            let e = format!("Not a UnixSocketStream locator: {:?}", locator);
            log::debug!("{}", e);
            "None".to_string()
        }
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocatorUnixSocketStream(PathBuf);

impl FromStr for LocatorUnixSocketStream {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr = match PathBuf::from(s).to_str() {
            Some(path) => Ok(PathBuf::from(path)),
            None => {
                let e = format!("Invalid UnixSocketStream locator: {:?}", s);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
        };
        addr.map(LocatorUnixSocketStream)
    }
}

impl fmt::Display for LocatorUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.0.to_str().unwrap_or("None");
        write!(f, "{}", path)?;
        Ok(())
    }
}

/*************************************/
/*            PROPERTY               */
/*************************************/
pub type LocatorPropertyUnixSocketStream = ();

/*************************************/
/*              LINK                 */
/*************************************/
pub struct LinkUnixSocketStream {
    // The underlying socket as returned from the async-std library
    socket: UnixStream,
    // The Unix domain socket source path
    src_path: String,
    // The Unix domain socker destination path (random UUIDv4)
    dst_path: String,
}

impl LinkUnixSocketStream {
    fn new(socket: UnixStream, src_path: String, dst_path: String) -> LinkUnixSocketStream {
        LinkUnixSocketStream {
            socket,
            src_path,
            dst_path,
        }
    }

    pub(crate) async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UnixSocketStream link: {}", self);
        // Close the underlying UnixSocketStream socket
        let res = self.socket.shutdown(Shutdown::Both);
        log::trace!("UnixSocketStream link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string(),
            })
        })
    }

    pub(crate) async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        (&self.socket).write(buffer).await.map_err(|e| {
            let e = format!("Write error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    pub(crate) async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        (&self.socket).write_all(buffer).await.map_err(|e| {
            let e = format!("Write error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    pub(crate) async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        (&self.socket).read(buffer).await.map_err(|e| {
            let e = format!("Read error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    pub(crate) async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        (&self.socket).read_exact(buffer).await.map_err(|e| {
            let e = format!("Read error on UnixSocketStream link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    #[inline(always)]
    pub(crate) fn get_src(&self) -> Locator {
        Locator::UnixSocketStream(LocatorUnixSocketStream(PathBuf::from(
            self.src_path.clone(),
        )))
    }

    #[inline(always)]
    pub(crate) fn get_dst(&self) -> Locator {
        Locator::UnixSocketStream(LocatorUnixSocketStream(PathBuf::from(
            self.dst_path.clone(),
        )))
    }

    #[inline(always)]
    pub(crate) fn get_mtu(&self) -> usize {
        *UNIXSOCKSTREAM_DEFAULT_MTU
    }

    #[inline(always)]
    pub(crate) fn is_reliable(&self) -> bool {
        true
    }

    #[inline(always)]
    pub(crate) fn is_streamed(&self) -> bool {
        true
    }
}

impl Drop for LinkUnixSocketStream {
    fn drop(&mut self) {
        // Close the underlying UnixSocketStream socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for LinkUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_path, self.dst_path)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixSocketStream")
            .field("src", &self.src_path)
            .field("dst", &self.dst_path)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnixSocketStream {
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
    lock_fd: RawFd,
}

impl ListenerUnixSocketStream {
    fn new(
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
        lock_fd: RawFd,
    ) -> ListenerUnixSocketStream {
        ListenerUnixSocketStream {
            active,
            signal,
            handle,
            lock_fd,
        }
    }
}

pub struct LinkManagerUnixSocketStream {
    manager: SessionManager,
    listener: Arc<RwLock<HashMap<String, ListenerUnixSocketStream>>>,
}

impl LinkManagerUnixSocketStream {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerUnixSocketStream {
    async fn new_link(&self, locator: &Locator, _ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let path = get_unix_path(locator)?;

        // Create the UnixSocketStream connection
        let stream = UnixStream::connect(&path).await.map_err(|e| {
            let e = format!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::Other { descr: e })
        })?;

        let src_addr = stream.local_addr().map_err(|e| {
            let e = format!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // We do need the dst_addr value, we just need to check that is valid
        let _dst_addr = stream.peer_addr().map_err(|e| {
            let e = format!(
                "Can not create a new UnixSocketStream link bound to {:?}: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_path = match src_addr.as_pathname() {
            Some(path) => PathBuf::from(path),
            None => {
                let e = format!(
                    "Can not create a new UnixSocketStream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                PathBuf::from(format!("{}", Uuid::new_v4()))
            }
        };

        let local_path_str = local_path
            .to_str()
            .ok_or_else(|| {
                let e = format!(
                    "Can not create a new UnixSocketStream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?
            .to_string();

        let remote_path_str = path
            .to_str()
            .ok_or_else(|| {
                let e = format!(
                    "Can not create a new UnixSocketStream link bound to {:?}",
                    path
                );
                log::warn!("{}", e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?
            .to_string();

        let link = Arc::new(LinkUnixSocketStream::new(
            stream,
            local_path_str,
            remote_path_str,
        ));

        Ok(Link::UnixSocketStream(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        _ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let path = get_unix_path_as_string(locator);

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
        let lock_file_path = format!("{}.lock", path);

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
            let e = format!(
                "Can not create a new UnixSocketStream listener on {} - Unable to open lock file: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // We try to acquire the lock
        nix::fcntl::flock(lock_fd, nix::fcntl::FlockArg::LockExclusiveNonblock).map_err(|e| {
            let _ = nix::unistd::close(lock_fd);
            let e = format!(
                "Can not create a new UnixSocketStream listener on {} - Unable to acquire look: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        //Lock is acquired we can remove the socket file
        // If the file does not exist this would return an error.
        // We are not interested if the file was not existing.
        let _ = remove_file(path.clone());

        // Bind the Unix socket
        let socket = UnixListener::bind(&path).await.map_err(|e| {
            let e = format!(
                "Can not create a new UnixSocketStream listener on {}: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            let e = format!(
                "Can not create a new UnixSocketStream listener on {}: {}",
                path, e
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_path = PathBuf::from(local_addr.as_pathname().ok_or_else(|| {
            let e = format!("Can not create a new UnixSocketStream listener on {}", path);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?);

        let local_path_str = match local_path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => "None".to_string(),
        };

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_task(socket, c_active, c_signal, c_manager).await
        });

        let listener = ListenerUnixSocketStream::new(active, signal, handle, lock_fd);
        zasyncwrite!(self.listener).insert(local_path_str.clone(), listener);

        Ok(Locator::UnixSocketStream(LocatorUnixSocketStream(
            local_path,
        )))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let path = get_unix_path_as_string(locator);

        // Stop the listener
        match zasyncwrite!(self.listener).remove(&path) {
            Some(listener) => {
                // Send the stop signal
                listener.active.store(false, Ordering::Release);
                listener.signal.trigger();
                let _ = listener.handle.await;

                //Release the lock
                let _ = nix::fcntl::flock(listener.lock_fd, nix::fcntl::FlockArg::UnlockNonblock);
                let _ = nix::unistd::close(listener.lock_fd);
                let _ = remove_file(path.clone());

                // Remove the Unix Domain Socket file
                let lock_file_path = format!("{}.lock", path);
                let res = remove_file(lock_file_path);
                log::trace!("UnixSocketStream Domain Socket removal result: {:?}", res);
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the UnixSocketStream listener because it has not been found: {}",
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
            .map(|x| Locator::UnixSocketStream(LocatorUnixSocketStream(PathBuf::from(x))))
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
        locators
            .into_iter()
            .map(|x| Locator::UnixSocketStream(LocatorUnixSocketStream(x)))
            .collect()
    }
}

async fn accept_task(
    socket: UnixListener,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: SessionManager,
) -> ZResult<()> {
    enum Action {
        Accept(UnixStream),
        Stop,
    }

    async fn accept(socket: &UnixListener) -> ZResult<Action> {
        let (stream, _) = socket.accept().await.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })?;
        Ok(Action::Accept(stream))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = format!("Can not accept UnixSocketStream connections: {}", e);
        log::warn!("{}", e);
        zerror2!(ZErrorKind::IoError { descr: e })
    })?;

    let local_path = PathBuf::from(src_addr.as_pathname().ok_or_else(|| {
        let e = format!(
            "Can not create a new UnixSocketStream link bound to {:?}",
            src_addr
        );
        log::warn!("{}", e);
        zerror2!(ZErrorKind::IoError { descr: e })
    })?);

    let src_path = local_path
        .to_str()
        .ok_or_else(|| {
            let e = format!(
                "Can not create a new UnixSocketStream link bound to {:?}",
                src_addr
            );
            log::warn!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })?
        .to_string();

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
        let link = Arc::new(LinkUnixSocketStream::new(
            stream,
            src_path.clone(),
            dst_path,
        ));

        // Communicate the new link to the initial session manager
        manager
            .handle_new_link(Link::UnixSocketStream(link), None)
            .await;
    }

    Ok(())
}
