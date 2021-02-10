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
#[cfg(feature = "transport_tcp")]
mod tcp;
#[cfg(feature = "transport_tls")]
pub mod tls;
#[cfg(feature = "transport_udp")]
mod udp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
mod unixsock_stream;

use crate::io::{RBuf, WBuf};
use crate::proto::SessionMessage;
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

const WBUF_SIZE: usize = 64;

#[derive(Clone)]
pub struct Link(Arc<dyn LinkTrait + Send + Sync>);

impl Link {
    fn new(link: Arc<dyn LinkTrait + Send + Sync>) -> Link {
        Self(link)
    }

    pub async fn write_session_message(&self, msg: SessionMessage) -> ZResult<()> {
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        if self.is_streamed() {
            // Reserve 16 bits to write the length
            wbuf.write_bytes(&[0u8, 0u8]);
        }
        // Serialize the message
        wbuf.write_session_message(&msg);
        if self.is_streamed() {
            // Write the length on the first 16 bits
            let length: u16 = wbuf.len() as u16 - 2;
            let bits = wbuf.get_first_slice_mut(..2);
            bits.copy_from_slice(&length.to_le_bytes());
        }
        let mut buffer = vec![0u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        self.write_all(&buffer).await
    }

    pub async fn read_session_message(&self) -> ZResult<Vec<SessionMessage>> {
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

        let mut rbuf = RBuf::from(buffer);
        let mut messages: Vec<SessionMessage> = Vec::with_capacity(1);
        while rbuf.can_read() {
            match rbuf.read_session_message() {
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

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        (self.0.get_src() == other.0.get_src()) && (self.0.get_dst() == other.0.get_dst())
    }
}

impl Hash for Link {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.get_src().hash(state);
        self.0.get_dst().hash(state);
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.0.get_src(), self.0.get_dst())
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.0.get_src())
            .field("dst", &self.0.get_dst())
            .field("mtu", &self.0.get_mtu())
            .field("is_reliable", &self.0.is_reliable())
            .field("is_streamed", &self.0.is_streamed())
            .finish()
    }
}

/*************************************/
/*           LINK MANAGER            */
/*************************************/
#[async_trait]
pub trait LinkManagerTrait {
    async fn new_link(&self, dst: &Locator, property: Option<&LocatorProperty>) -> ZResult<Link>;
    async fn new_listener(
        &self,
        locator: &Locator,
        property: Option<&LocatorProperty>,
    ) -> ZResult<Locator>;
    async fn del_listener(&self, locator: &Locator) -> ZResult<()>;
    async fn get_listeners(&self) -> Vec<Locator>;
    async fn get_locators(&self) -> Vec<Locator>;
}

pub type LinkManager = Arc<dyn LinkManagerTrait + Send + Sync>;
