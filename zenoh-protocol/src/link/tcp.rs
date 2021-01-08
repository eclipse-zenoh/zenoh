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
use async_std::channel::{bounded, Receiver, Sender};
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, RwLock};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::net::Shutdown;
use std::time::Duration;

use super::{Link, LinkManagerTrait, LinkTrait, Locator};
use crate::session::SessionManagerInitial;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasyncread, zasyncwrite, zerror, zerror2};

// Default MTU (TCP PDU) in bytes.
// NOTE: Since TCP is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TCP MTU is constrained to
//       2^16 + 1 bytes (i.e., 65537).
const TCP_MAX_MTU: usize = 65_537;

zconfigurable! {
    // Default MTU (TCP PDU) in bytes.
    static ref TCP_DEFAULT_MTU: usize = TCP_MAX_MTU;
    // Size of buffer used to read from socket.
    static ref TCP_READ_BUFFER_SIZE: usize = 2*TCP_MAX_MTU;

    // Size of the vector used to deserialize the messages.
    static ref TCP_READ_MESSAGES_VEC_SIZE: usize = 32;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TCP_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TCP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[macro_export]
macro_rules! get_tcp_addr {
    ($locator:expr) => {
        match $locator {
            Locator::Tcp(addr) => addr,
            _ => {
                let e = format!("Not a TCP locator: {}", $locator);
                log::debug!("{}", e);
                return zerror!(ZErrorKind::InvalidLocator { descr: e });
            }
        }
    };
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct Tcp {
    // The underlying socket as returned from the async-std library
    socket: TcpStream,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    // The source Zenoh locator of this link (locator used on the local host)
    src_locator: Locator,
    // The destination Zenoh locator of this link (locator used on the local host)
    dst_locator: Locator,
}

impl Tcp {
    fn new(socket: TcpStream, src_addr: SocketAddr, dst_addr: SocketAddr) -> Tcp {
        // Set the TCP nodelay option
        if let Err(err) = socket.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TCP link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }
        // Set the TCP linger option
        if let Err(err) = zenoh_util::net::set_linger(
            &socket,
            Some(Duration::from_secs(
                (*TCP_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TCP link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tcp object
        Tcp {
            socket,
            src_addr,
            dst_addr,
            src_locator: Locator::Tcp(src_addr),
            dst_locator: Locator::Tcp(dst_addr),
        }
    }
}

#[async_trait]
impl LinkTrait for Tcp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TCP link: {}", self);
        // Close the underlying TCP socket
        let res = self.socket.shutdown(Shutdown::Both);
        log::trace!("TCP link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IOError {
                descr: format!("{}", e),
            })
        })
    }

    async fn send(&self, buffer: &[u8]) -> ZResult<usize> {
        let res = (&self.socket).write_all(buffer).await;
        if let Err(e) = res {
            log::trace!("Transmission error on TCP link {}: {}", self, e);
            return zerror!(ZErrorKind::IOError {
                descr: format!("{}", e)
            });
        }

        Ok(buffer.len())
    }

    async fn receive(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match (&self.socket).read(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Reception error on TCP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    fn get_src(&self) -> Locator {
        self.src_locator.clone()
    }

    fn get_dst(&self) -> Locator {
        self.dst_locator.clone()
    }

    fn get_mtu(&self) -> usize {
        *TCP_DEFAULT_MTU
    }

    fn is_reliable(&self) -> bool {
        true
    }

    fn is_streamed(&self) -> bool {
        true
    }
}

// async fn read_task(link: Arc<Tcp>, stop: Receiver<()>) {
// let read_loop = async {
//     // The link object to be passed to the transport
//     let lobj: Link = Link::new(link.clone());
//     // Acquire the lock on the transport
//     let mut guard = zasynclock!(link.transport);

//     // // The RBuf to read a message batch onto
//     let mut rbuf = RBuf::new();

//     // The buffer allocated to read from a single syscall
//     let mut buffer = vec![0u8; *TCP_READ_BUFFER_SIZE];

//     // The vector for storing the deserialized messages
//     let mut messages: Vec<SessionMessage> = Vec::with_capacity(*TCP_READ_MESSAGES_VEC_SIZE);

//     // An example of the received buffer and the correspoding indexes is:
//     //
//     //  0 1 2 3 4 5 6 7  ..  n 0 1 2 3 4 5 6 7      k 0 1 2 3 4 5 6 7      x
//     // +-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+
//     // | L | First batch      | L | Second batch     | L | Incomplete batch |
//     // +-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+
//     //
//     // - Decoding Iteration 0:
//     //      r_l_pos = 0; r_s_pos = 2; r_e_pos = n;
//     //
//     // - Decoding Iteration 1:
//     //      r_l_pos = n; r_s_pos = n+2; r_e_pos = n+k;
//     //
//     // - Decoding Iteration 2:
//     //      r_l_pos = n+k; r_s_pos = n+k+2; r_e_pos = n+k+x;
//     //
//     // In this example, Iteration 2 will fail since the batch is incomplete and
//     // fewer bytes than the ones indicated in the length are read. The incomplete
//     // batch is hence stored in a RBuf in order to read more bytes from the socket
//     // and deserialize a complete batch. In case it is not possible to read at once
//     // the 2 bytes indicating the message batch length (i.e., only the first byte is
//     // available), the first byte is copied at the beginning of the buffer and more
//     // bytes are read in the next iteration.

//     // The read position of the length bytes in the buffer
//     let mut r_l_pos: usize;
//     // The start read position of the message bytes in the buffer
//     let mut r_s_pos: usize;
//     // The end read position of the messages bytes in the buffer
//     let mut r_e_pos: usize;
//     // The write position in the buffer
//     let mut w_pos: usize = 0;

//     // Keep track of the number of bytes still to read for incomplete message batches
//     let mut left_to_read: usize = 0;

//     // Macro to handle a link error
//     macro_rules! zlinkerror {
//         ($notify:expr) => {
//             // Close the underlying TCP socket
//             let _ = link.socket.shutdown(Shutdown::Both);
//             // Delete the link from the manager
//             let _ = link.manager.del_link(&link.src_addr, &link.dst_addr).await;
//             if $notify {
//                 // Notify the transport
//                 let _ = guard.link_err(&lobj).await;
//             }
//             // Exit
//             return Ok(());
//         };
//     }

//     // Macro to add a slice to the RBuf
//     macro_rules! zaddslice {
//         ($start:expr, $end:expr) => {
//             let tot = $end - $start;
//             let mut slice = Vec::with_capacity(tot);
//             slice.extend_from_slice(&buffer[$start..$end]);
//             rbuf.add_slice(ArcSlice::new(Arc::new(slice), 0, tot));
//         };
//     }

//     // Macro for deserializing the messages
//     macro_rules! zdeserialize {
//         () => {
//             // Deserialize all the messages from the current RBuf
//             while rbuf.can_read() {
//                 match rbuf.read_session_message() {
//                     Some(msg) => messages.push(msg),
//                     None => {
//                         zlinkerror!(true);
//                     }
//                 }
//             }

//             for msg in messages.drain(..) {
//                 let res = guard.receive_message(&lobj, msg).await;
//                 // Enforce the action as instructed by the upper logic
//                 match res {
//                     Ok(action) => match action {
//                         Action::Read => {}
//                         Action::ChangeTransport(transport) => {
//                             log::trace!("Change transport on TCP link: {}", link);
//                             *guard = transport
//                         }
//                         Action::Close => {
//                             log::trace!("Closing TCP link: {}", link);
//                             zlinkerror!(false);
//                         }
//                     },
//                     Err(e) => {
//                         log::trace!("Closing TCP link {}: {}", link, e);
//                         zlinkerror!(false);
//                     }
//                 }
//             }
//         };
//     }
//     log::trace!("Ready to read from TCP link: {}", link);
//     loop {
//         // Async read from the TCP socket
//         match (&link.socket).read(&mut buffer[w_pos..]).await {
//             Ok(mut n) => {
//                 if n == 0 {
//                     // Reading 0 bytes means error
//                     log::debug!("Zero bytes reading on TCP link: {}", link);
//                     zlinkerror!(true);
//                 }

//                 // If we had a w_pos different from 0, it means we add an incomplete length reading
//                 // in the previous iteration: we have only read 1 byte instead of 2.
//                 if w_pos != 0 {
//                     // Update the number of read bytes by adding the bytes we have already read in
//                     // the previous iteration. "n" now is the index pointing to the last valid
//                     // position in the buffer.
//                     n += w_pos;
//                     // Reset the write index
//                     w_pos = 0;
//                 }

//                 // Reset the read length index
//                 r_l_pos = 0;

//                 // Check if we had an incomplete message batch
//                 if left_to_read > 0 {
//                     // Check if still we haven't read enough bytes
//                     if n < left_to_read {
//                         // Update the number of bytes still to read;
//                         left_to_read -= n;
//                         // Copy the relevant buffer slice in the RBuf
//                         zaddslice!(0, n);
//                         // Keep reading from the socket
//                         continue;
//                     }
//                     // We are ready to decode a complete message batch
//                     // Copy the relevant buffer slice in the RBuf
//                     zaddslice!(0, left_to_read);
//                     // Read the batch
//                     zdeserialize!();
//                     // Update the read length index
//                     r_l_pos = left_to_read;
//                     // Reset the remaining bytes to read
//                     left_to_read = 0;

//                     // Check if we have completely read the batch
//                     if buffer[r_l_pos..n].is_empty() {
//                         // Reset the RBuf
//                         rbuf.clear();
//                         // Keep reading from the socket
//                         continue;
//                     }
//                 }

//                 // Loop over all the buffer which may contain multiple message batches
//                 loop {
//                     // Compute the total number of bytes we have read
//                     let read = buffer[r_l_pos..n].len();
//                     // Check if we have read the 2 bytes necessary to decode the message length
//                     if read < 2 {
//                         // Copy the bytes at the beginning of the buffer
//                         buffer.copy_within(r_l_pos..n, 0);
//                         // Update the write index
//                         w_pos = read;
//                         // Keep reading from the socket
//                         break;
//                     }
//                     // We have read at least two bytes in the buffer, update the read start index
//                     r_s_pos = r_l_pos + 2;
//                     // Read the length as litlle endian from the buffer (array of 2 bytes)
//                     let length: [u8; 2] = buffer[r_l_pos..r_s_pos].try_into().unwrap();
//                     // Decode the total amount of bytes that we are expected to read
//                     let to_read = u16::from_le_bytes(length) as usize;

//                     // Check if we have really something to read
//                     if to_read == 0 {
//                         // Keep reading from the socket
//                         break;
//                     }
//                     // Compute the number of useful bytes we have actually read
//                     let read = buffer[r_s_pos..n].len();

//                     if read == 0 {
//                         // The buffer might be empty in case of having read only the two bytes
//                         // of the length and no additional bytes are left in the reading buffer
//                         left_to_read = to_read;
//                         // Keep reading from the socket
//                         break;
//                     } else if read < to_read {
//                         // We haven't read enough bytes for a complete batch, so
//                         // we need to store the bytes read so far and keep reading

//                         // Update the number of bytes we still have to read to
//                         // obtain a complete message batch for decoding
//                         left_to_read = to_read - read;

//                         // Copy the buffer in the RBuf if not empty
//                         zaddslice!(r_s_pos, n);

//                         // Keep reading from the socket
//                         break;
//                     }

//                     // We have at least one complete message batch we can deserialize
//                     // Compute the read end index of the message batch in the buffer
//                     r_e_pos = r_s_pos + to_read;

//                     // Copy the relevant buffer slice in the RBuf
//                     zaddslice!(r_s_pos, r_e_pos);
//                     // Deserialize the batch
//                     zdeserialize!();

//                     // Reset the current RBuf
//                     rbuf.clear();
//                     // Reset the remaining bytes to read
//                     left_to_read = 0;

//                     // Check if we are done with the current reading buffer
//                     if buffer[r_e_pos..n].is_empty() {
//                         // Keep reading from the socket
//                         break;
//                     }

//                     // Update the read length index to read the next message batch
//                     r_l_pos = r_e_pos;
//                 }
//             }
//             Err(e) => {
//                 log::debug!("Reading error on TCP link {}: {}", link, e);
//                 zlinkerror!(true);
//             }
//         }
//     }
// };

// // Execute the read loop
// let _ = read_loop.race(stop.recv()).await;
// }

impl Drop for Tcp {
    fn drop(&mut self) {
        // Close the underlying TCP socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for Tcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for Tcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tcp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerTcpInner {
    socket: Arc<TcpListener>,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerTcpInner {
    fn new(socket: Arc<TcpListener>) -> ListenerTcpInner {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerTcpInner {
            socket,
            sender,
            receiver,
            barrier,
        }
    }
}

pub struct LinkManagerTcp {
    initial: Arc<SessionManagerInitial>,
    listener: Arc<RwLock<HashMap<SocketAddr, Arc<ListenerTcpInner>>>>,
}

impl LinkManagerTcp {
    pub(crate) fn new(initial: Arc<SessionManagerInitial>) -> Self {
        Self {
            initial,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerTcp {
    async fn new_link(&self, locator: &Locator) -> ZResult<Link> {
        let dst_addr = get_tcp_addr!(locator);

        // Create the TCP connection
        let stream = match TcpStream::connect(dst_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }
        };
        // Create a new link object
        let src_addr = match stream.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let dst_addr = match stream.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let link = Arc::new(Tcp::new(stream, src_addr, dst_addr));
        let link = Link::new(link);

        Ok(link)
    }

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let addr = get_tcp_addr!(locator);

        // Bind the TCP socket
        let socket = match TcpListener::bind(addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                let e = format!("Can not create a new TCP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let listener = Arc::new(ListenerTcpInner::new(socket));
        // Update the list of active listeners on the manager
        zasyncwrite!(self.listener).insert(local_addr, listener.clone());

        // Spawn the accept loop for the listener
        let c_listeners = self.listener.clone();
        let c_addr = local_addr;
        let c_initial = self.initial.clone();
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_task(listener, c_initial).await;
            // Delete the listener from the manager
            zasyncwrite!(c_listeners).remove(&c_addr);
        });

        Ok(Locator::Tcp(local_addr))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_tcp_addr!(locator);

        // Stop the listener
        match zasyncwrite!(self.listener).remove(&addr) {
            Some(listener) => {
                // Send the stop signal
                if listener.sender.send(()).await.is_ok() {
                    // Wait for the accept loop to be stopped
                    listener.barrier.wait().await;
                }
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the TCP listener because it has not been found: {}",
                    addr
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn get_listeners(&self) -> Vec<Locator> {
        zasyncread!(self.listener)
            .keys()
            .map(|x| Locator::Tcp(*x))
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];
        for addr in zasyncread!(self.listener).keys() {
            if addr.ip() == std::net::Ipv4Addr::new(0, 0, 0, 0) {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && ipaddr.is_ipv4() {
                                locators.push(SocketAddr::new(ipaddr, addr.port()));
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else {
                locators.push(*addr)
            }
        }
        locators.into_iter().map(Locator::Tcp).collect()
    }
}

async fn accept_task(listener: Arc<ListenerTcpInner>, initial: Arc<SessionManagerInitial>) {
    // The accept future
    let accept_loop = async {
        log::trace!(
            "Ready to accept TCP connections on: {:?}",
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
                    task::sleep(Duration::from_micros(*TCP_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }
            };
            // Get the source and destination TCP addresses
            let src_addr = match stream.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TCP connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };
            let dst_addr = match stream.peer_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TCP connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };

            log::debug!("Accepted TCP connection on {:?}: {:?}", src_addr, dst_addr);
            // Create the new link object
            let link = Arc::new(Tcp::new(stream, src_addr, dst_addr));

            // Communicate the new link to the initial session manager
            initial.handle_new_link(Link::new(link)).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
