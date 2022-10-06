//
// Copyright (c) 2022 ZettaScale Technology
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
use zenoh_buffers::{buffer::CopyBuffer, WBuf, ZBufReader};
use zenoh_core::zcheck;
use zenoh_protocol_core::whatami::WhatAmIMatcher;

/// # Scout message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The SCOUT message can be sent at any point in time to solicit HELLO messages from matching parties.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|W|I|  SCOUT  |
/// +-+-+-+-+-------+
/// ~      what     ~ if W==1 -- Otherwise implicitly scouting for Routers
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub what: Option<WhatAmIMatcher>,
    pub zid_request: bool,
}

impl Header for Scout {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::SCOUT;
        if self.zid_request {
            header |= tmsg::flag::I;
        }
        if self.what.is_some() {
            header |= tmsg::flag::W;
        }
        header
    }
}

pub trait ScoutRead {
    fn read_scout(&mut self, header: u8) -> Option<TransportBody>;
}

pub trait ScoutWrite {
    fn write_scout(&mut self, scout: &Scout) -> bool;
}

impl ScoutRead for ZBufReader<'_> {
    fn read_scout(&mut self, header: u8) -> Option<TransportBody> {
        let zid_request = imsg::has_flag(header, tmsg::flag::I);
        let what = if imsg::has_flag(header, tmsg::flag::W) {
            WhatAmIMatcher::try_from(self.read_zint()?)
        } else {
            None
        };
        // Some(TransportBody::Scout(Scout { what, zid_request }))
        None
    }
}

impl ScoutWrite for WBuf {
    fn write_scout(&mut self, scout: &Scout) -> bool {
        zcheck!(self.write_byte(scout.header()).is_some());
        match scout.what {
            Some(w) => self.write_zint(w.into()),
            None => true,
        }
    }
}
