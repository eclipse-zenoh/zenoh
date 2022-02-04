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
use super::{TransportId, TransportProto};
use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::message::extensions::{has_more, ZExt, ZExtUnknown};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;

/// # Close message
///
/// The [`Close`] message is sent in any of the following two cases:
///     1) in response to an INIT or OPEN message which are not accepted;
///     2) at any time to arbitrarly close the transport with the corresponding peer.
///
/// The [`Close`] message flow is the following:
///
/// ```text
///     A                   B
///     |       CLOSE       |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Close`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X|  CLOSE  |
/// +-+-+-+---------+
/// |     reason    |
/// +---------------+
/// ~  [CloseExts]  ~ if Flag(Z)==1
/// +---------------+
/// ```
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CloseReason {
    Generic = 0x01,
    Unsupported = 0x02,
    Invalid = 0x03,
    MaxSessions = 0x04,
    MaxLinks = 0x05,
    LeaseExpired = 0x06,
}

impl CloseReason {
    const fn id(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for CloseReason {
    type Error = ();

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        const GENERIC: u8 = CloseReason::Generic.id();
        const UNSUPPORTED: u8 = CloseReason::Unsupported.id();
        const INVALID: u8 = CloseReason::Invalid.id();
        const MAX_SESSIONS: u8 = CloseReason::MaxSessions.id();
        const MAX_LINKS: u8 = CloseReason::MaxLinks.id();
        const LEASE_EXPIRED: u8 = CloseReason::LeaseExpired.id();

        match b {
            GENERIC => Ok(Self::Generic),
            UNSUPPORTED => Ok(Self::Unsupported),
            INVALID => Ok(Self::Invalid),
            MAX_SESSIONS => Ok(Self::MaxSessions),
            MAX_LINKS => Ok(Self::MaxLinks),
            LEASE_EXPIRED => Ok(Self::LeaseExpired),
            _ => Err(()),
        }
    }
}

impl Default for CloseReason {
    fn default() -> Self {
        Self::Generic
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub reason: CloseReason,
    pub exts: CloseExts,
}

impl Close {
    // Header flags
    // pub const FLAG_X: u8 = 1 << 5; // Reserved for future use
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(reason: CloseReason) -> Self {
        Self {
            reason,
            exts: CloseExts::default(),
        }
    }
}

impl ZMessage for Close {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Close.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if has_exts {
            header |= Close::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write(self.reason.id()));

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Close> {
        let reason = CloseReason::try_from(zbuf.read()?).ok()?;

        let exts = if has_flag(header, Close::FLAG_Z) {
            CloseExts::read(zbuf)?
        } else {
            CloseExts::default()
        };

        Some(Close { reason, exts })
    }
}

/// # Close message extensions
type CloseExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, PartialEq)]
pub struct CloseExts;

impl CloseExts {
    fn is_empty(&self) -> bool {
        true
    }

    fn write(&self, _wbuf: &mut WBuf) -> bool {
        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<CloseExts> {
        let exts = CloseExts::default();

        loop {
            let header = zbuf.read()?;

            let _e: CloseExtUnk = ZExt::read(zbuf, header)?;

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
