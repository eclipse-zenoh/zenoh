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
mod locator;
mod manager;
#[cfg(feature = "transport_quic")]
pub mod quic;
#[cfg(feature = "transport_tcp")]
pub mod tcp;
#[cfg(feature = "transport_tls")]
pub mod tls;
#[cfg(feature = "transport_udp")]
pub mod udp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub mod unixsock_stream;

use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::proto::TransportMessage;
use async_std::sync::Arc;
use async_trait::async_trait;
pub use locator::*;
pub use manager::*;
use std::cmp::PartialEq;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*************************************/
/*              LINK                 */
/*************************************/
const WBUF_SIZE: usize = 64;

#[derive(Clone)]
pub struct Link(Arc<dyn LinkTrait + Send + Sync>);

#[async_trait]
pub trait LinkTrait {
    fn get_mtu(&self) -> usize;
    fn get_src(&self) -> Locator;
    fn get_dst(&self) -> Locator;
    fn is_reliable(&self) -> bool;
    fn is_streamed(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize>;
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()>;
    async fn close(&self) -> ZResult<()>;
}

impl Link {
    pub(crate) async fn write_transport_message(&self, msg: TransportMessage) -> ZResult<()> {
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        if self.is_streamed() {
            // Reserve 16 bits to write the length
            wbuf.write_bytes(&[0u8, 0u8]);
        }
        // Serialize the message
        wbuf.write_transport_message(&msg);
        if self.is_streamed() {
            // Write the length on the first 16 bits
            let length: u16 = wbuf.len() as u16 - 2;
            let bits = wbuf.get_first_slice_mut(..2);
            bits.copy_from_slice(&length.to_le_bytes());
        }
        let mut buffer = vec![0u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        self.0.write_all(&buffer).await
    }

    pub(crate) async fn read_transport_message(&self) -> ZResult<Vec<TransportMessage>> {
        // Read from the link
        let buffer = if self.is_streamed() {
            // Read and decode the message length
            let mut length_bytes = [0u8; 2];
            let _ = self.read_exact(&mut length_bytes).await?;
            let to_read = u16::from_le_bytes(length_bytes) as usize;
            // Read the message
            let mut buffer = vec![0u8; to_read];
            let _ = self.read_exact(&mut buffer).await?;
            buffer
        } else {
            // Read the message
            let mut buffer = vec![0u8; self.get_mtu()];
            let n = self.read(&mut buffer).await?;
            buffer.truncate(n);
            buffer
        };

        let mut zbuf = ZBuf::from(buffer);
        let mut messages: Vec<TransportMessage> = Vec::with_capacity(1);
        while zbuf.can_read() {
            match zbuf.read_transport_message() {
                Some(msg) => messages.push(msg),
                None => {
                    let e = format!("Decoding error on link: {}", self);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }
        }

        Ok(messages)
    }
}

impl Deref for Link {
    type Target = Arc<dyn LinkTrait + Send + Sync>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.get_src() == other.get_src() && self.get_dst() == other.get_dst()
    }
}

impl Hash for Link {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_src().hash(state);
        self.get_dst().hash(state);
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.get_src(), self.get_dst())
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.get_src())
            .field("dst", &self.get_dst())
            .field("mtu", &self.get_mtu())
            .field("is_reliable", &self.is_reliable())
            .field("is_streamed", &self.is_streamed())
            .finish()
    }
}

impl From<Arc<dyn LinkTrait + Send + Sync>> for Link {
    fn from(link: Arc<dyn LinkTrait + Send + Sync>) -> Link {
        Link(link)
    }
}
