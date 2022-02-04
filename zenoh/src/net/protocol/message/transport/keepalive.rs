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

/// # KeepAlive message
///
/// The [`KeepAlive`] message SHOULD be sent periodically to avoid the expiration of the
/// link lease period. A [`KeepAlive`] message MAY NOT be sent on the link if some other
/// data has been transmitted on the same link during the last keep alive interval.
///
/// The [`KeepAlive`] message flow is the following:
///
/// ```text
/// A                   B
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |<------------------|
/// |                   |
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |<------------------|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// ```
///
/// NOTE: In order to consider eventual packet loss, transmission latency and jitter, the time
///       interval between two subsequent [`KeepAlive`] messages SHOULD be set to one fourth of
///       the lease time. This is in-line with the ITU-T G.8013/Y.1731 specification on continous
///       connectivity check which considers a link as failed when no messages are received in
///       3.5 times the target keep alive interval.
///
/// The [`KeepAlive`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X| KALIVE  |
/// +-+-+-+---------+
/// ~  [KAliveExts] ~ if Flag(Z)==1
/// +---------------+
/// ```
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Clone, PartialEq, Default, Debug)]
pub struct KeepAlive {
    pub exts: KeepAliveExts,
}

impl KeepAlive {
    // Header flags
    // pub const FLAG_X: u8 = 1 << 5; // Reserved for future use
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new() -> Self {
        Self {
            exts: KeepAliveExts::default(),
        }
    }
}

impl ZMessage for KeepAlive {
    type Proto = TransportProto;
    const ID: u8 = TransportId::KeepAlive.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if has_exts {
            header |= KeepAlive::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<KeepAlive> {
        let exts = if has_flag(header, KeepAlive::FLAG_Z) {
            KeepAliveExts::read(zbuf)?
        } else {
            KeepAliveExts::default()
        };

        Some(KeepAlive { exts })
    }
}

/// # KeepAlive message extensions
type KeepAliveExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, PartialEq)]
pub struct KeepAliveExts;

impl KeepAliveExts {
    fn is_empty(&self) -> bool {
        true
    }

    fn write(&self, _wbuf: &mut WBuf) -> bool {
        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<KeepAliveExts> {
        let exts = KeepAliveExts::default();

        loop {
            let header = zbuf.read()?;

            let _e: KeepAliveExtUnk = ZExt::read(zbuf, header)?;

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
