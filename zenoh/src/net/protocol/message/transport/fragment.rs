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
use super::frame::ZExtQoS;
use super::{TransportId, TransportProto};
use crate::net::protocol::core::{Reliability, ZInt};
use crate::net::protocol::io::{WBuf, ZBuf, ZSlice};
use crate::net::protocol::message::extensions::{eid, has_more, ZExt, ZExtUnknown};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;

/// # Fragment message
///
/// The [`Fragment`] message is used to transmit on the wire large [`crate::net::protocol::message::ZenohMessage`]
/// that require fragmentation because they are larger thatn the maximum batch size
/// (i.e. 2^16-1) and/or the link MTU.
///
/// The [`Fragment`] message flow is the following:
///
/// ```text
///     A                   B
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT         |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Fragment`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - R: Reliable       If R==1 it concerns the reliable channel, else the best-effort channel
/// - M: More           If M==1 then other fragments will follow
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|R| FRAGMENT|
/// +-+-+-+---------+
/// %    seq num    %
/// +---------------+
/// ~   [FragExts]  ~ if Flag(Z)==1
/// +---------------+
/// ~      [u8]     ~
/// +---------------+
/// ```
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Clone, PartialEq, Debug)]
pub struct Fragment {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub has_more: bool,
    pub payload: ZSlice,
    pub exts: FragmentExts,
}

impl Fragment {
    // Header flags
    pub const FLAG_R: u8 = 1 << 5;
    pub const FLAG_M: u8 = 1 << 6;
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(reliability: Reliability, sn: ZInt, has_more: bool, payload: ZSlice) -> Self {
        Self {
            reliability,
            sn,
            has_more,
            payload,
            exts: FragmentExts::default(),
        }
    }

    pub fn write_header(
        wbuf: &mut WBuf,
        reliability: Reliability,
        sn: ZInt,
        has_more: bool,
        exts: &FragmentExts,
    ) -> bool {
        // Compute extensions
        let has_exts = !exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if let Reliability::Reliable = reliability {
            header |= Fragment::FLAG_R;
        }
        if has_more {
            header |= Fragment::FLAG_M;
        }
        if has_exts {
            header |= Fragment::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write_zint(sn));

        // Write extensions
        if has_exts {
            zcheck!(exts.write(wbuf));
        }

        true
    }

    pub fn read_header(
        zbuf: &mut ZBuf,
        header: u8,
    ) -> Option<(Reliability, ZInt, bool, FragmentExts)> {
        let reliability = if has_flag(header, Fragment::FLAG_R) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };
        let has_more = has_flag(header, Fragment::FLAG_M);

        let sn = zbuf.read_zint()?;

        let exts = if has_flag(header, Fragment::FLAG_Z) {
            FragmentExts::read(zbuf)?
        } else {
            FragmentExts::default()
        };

        Some((reliability, sn, has_more, exts))
    }

    pub fn read_body(zbuf: &mut ZBuf) -> Option<ZSlice> {
        zbuf.read_zslice(zbuf.readable())
    }

    pub fn skip_body(zbuf: &mut ZBuf) -> Option<()> {
        match zbuf.skip_bytes(zbuf.readable()) {
            true => Some(()),
            false => None,
        }
    }
}

impl ZMessage for Fragment {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Fragment.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Write header
        zcheck!(Self::write_header(
            wbuf,
            self.reliability,
            self.sn,
            self.has_more,
            &self.exts
        ));
        // Write payload
        wbuf.write_zslice(self.payload.clone())
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Fragment> {
        // Read header
        let (reliability, sn, has_more, exts) = Fragment::read_header(zbuf, header)?;
        // Read payload
        let payload = Fragment::read_body(zbuf)?;

        Some(Fragment {
            reliability,
            sn,
            has_more,
            payload,
            exts,
        })
    }
}

/// # Fragment message extensions
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FragmentExtId {
    // 0x00: Reserved
    // 0x01: Reserved
    QoS = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for FragmentExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const QOS: u8 = FragmentExtId::QoS.id();

        match id {
            QOS => Ok(FragmentExtId::QoS),
            _ => Err(()),
        }
    }
}

impl FragmentExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

type FragmentExtQoS = ZExt<ZExtQoS<{ FragmentExtId::QoS.id() }>>;
type FragmentExtUnk = ZExt<ZExtUnknown>;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct FragmentExts {
    pub qos: Option<FragmentExtQoS>,
}

impl FragmentExts {
    fn is_empty(&self) -> bool {
        self.qos.is_none()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        if let Some(qos) = self.qos.as_ref() {
            zcheck!(qos.write(wbuf, false));
        }

        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<FragmentExts> {
        let mut exts = FragmentExts::default();

        loop {
            let header = zbuf.read()?;

            match FragmentExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    FragmentExtId::QoS => {
                        let e: FragmentExtQoS = ZExt::read(zbuf, header)?;
                        exts.qos = Some(e);
                    }
                },
                Err(_) => {
                    let _e: FragmentExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
