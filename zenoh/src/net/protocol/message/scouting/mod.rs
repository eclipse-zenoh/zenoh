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
mod hello;
mod scout;

use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::message::ZMessage;
pub use hello::*;
pub use scout::*;
use std::convert::TryFrom;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScoutingId {
    // 0x00: Reserved
    Scout = 0x01,
    Hello = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for ScoutingId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const SCT: u8 = ScoutingId::Scout.id();
        const HEL: u8 = ScoutingId::Hello.id();

        match id {
            SCT => Ok(ScoutingId::Scout),
            HEL => Ok(ScoutingId::Hello),
            _ => Err(()),
        }
    }
}

impl ScoutingId {
    const fn id(self) -> u8 {
        self as u8
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ScoutingProto;

/// # Scouting protocol
///
/// In zenoh, scouting is used to discover any other zenoh node in the surroundings when
/// operating in a LAN-like environment. This is achieved by exploiting multicast capabilities
/// of the underlying network.
///
/// In case of operating on a multicast UDP/IP network, the scouting works as follows:
/// - A zenoh node sends out a SCOUT message transported by a UDP datagram addressed to a given
///   IP multicast group that reach any other zenoh node participating in the same group.
/// - Upon receiving the SCOUT message, zenoh nodes reply back with an HELLO message carried by
///   a second UDP datagram containing the information (i.e., the set of locators) that can be
///   used to reach them (e.g., the TCP/IP address).
/// - The zenoh node who initially sent the SCOUT message may use the information on the locators
///   contained on the received HELLO messages to initiate a zenoh session with the discovered
///   zenoh nodes.
///
/// It's worth remarking that scouting in zenoh is related to discovering where other zenoh nodes
/// are (e.g. peers and/or routers) and not to the discovery of the resources actually being
/// published or subscribed. In other words, the scouting mechanism in zenoh helps at answering
/// the following questions:
/// - What are the zenoh nodes that are around in the same LAN-like network?
/// - What address I can use to reach them?
///
/// Finally, it is worth highlighting that in the case of operating o a network that does not
/// support multicast communication, scouting could be achieved through a different mechanism
/// like DNS, SDP, etc.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoutingBody {
    Scout(Scout),
    Hello(Hello),
}

impl From<Scout> for ScoutingBody {
    fn from(msg: Scout) -> Self {
        Self::Scout(msg)
    }
}

impl From<Hello> for ScoutingBody {
    fn from(msg: Hello) -> Self {
        Self::Hello(msg)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
    #[cfg(feature = "stats")]
    pub size: Option<NonZeroUsize>,
}

impl ScoutingMessage {
    #[allow(clippy::let_and_return)] // Necessary when feature "stats" is disabled
    pub fn write(&mut self, wbuf: &mut WBuf) -> bool {
        #[cfg(feature = "stats")]
        let start_written = wbuf.len();

        let res = match &mut self.body {
            ScoutingBody::Scout(scout) => scout.write(wbuf),
            ScoutingBody::Hello(hello) => hello.write(wbuf),
        };

        #[cfg(feature = "stats")]
        {
            let stop_written = wbuf.len();
            self.size = NonZeroUsize::new(stop_written - start_written);
        }

        res
    }

    pub fn read(zbuf: &mut ZBuf) -> Option<ScoutingMessage> {
        #[cfg(feature = "stats")]
        let start_readable = zbuf.readable();

        // Read the header
        let header = zbuf.read()?;

        // Read the message
        let mid = super::mid(header);
        let body = match ScoutingId::try_from(mid) {
            Ok(id) => match id {
                ScoutingId::Scout => ScoutingBody::Scout(Scout::read(zbuf, header)?),
                ScoutingId::Hello => ScoutingBody::Hello(Hello::read(zbuf, header)?),
            },
            Err(_) => {
                log::trace!("Scouting message with unknown ID: {}", mid);
                return None;
            }
        };

        #[cfg(feature = "stats")]
        let stop_readable = zbuf.readable();

        Some(ScoutingMessage {
            body,
            #[cfg(feature = "stats")]
            size: NonZeroUsize::new(start_readable - stop_readable),
        })
    }
}

impl<T: Into<ScoutingBody>> From<T> for ScoutingMessage {
    fn from(msg: T) -> Self {
        Self {
            body: msg.into(),
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}
