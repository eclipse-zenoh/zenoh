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
use crate::net::protocol::core::{NonZeroZInt, SeqNumBytes, Version, WhatAmI, ZenohId};
use crate::net::protocol::io::{WBuf, ZBuf, ZSlice};
use crate::net::protocol::message::extensions::{
    eid, has_more, ZExt, ZExtEmpty, ZExtPolicy, ZExtProperties, ZExtUnknown, ZExtZInt,
};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;

/// # Init message
///
/// The INIT message is sent on a specific Locator to initiate a transport with the zenoh node
/// associated with that Locator. The initiator MUST send an INIT message with the A flag set to 0.
/// If the corresponding zenohd node deems appropriate to accept the INIT message, the corresponding
/// peer MUST reply with an INIT message with the A flag set to 1. Alternatively, it MAY reply with
/// a [`super::Close`] message. For convenience, we call [`InitSyn`] and [`InitAck`] an INIT message
/// when the A flag is set to 0 and 1, respectively.
///
/// The [`InitSyn`]/[`InitAck`] message flow is the following:
///
/// ```text
///     A                   B
///     |      INIT SYN     |
///     |------------------>|
///     |                   |
///     |      INIT ACK     |
///     |<------------------|
///     |                   |
/// ```
///
/// The INIT message structure is defined as follows:
///
/// ```text
/// Flags:
/// - A: Ack            If A==0 then the message is an InitSyn else it is an InitAck
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|A|   INIT  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |X|X|X|sn_bs|wai| (#)(*)
/// +-+-+-+-----+---+
/// ~      <u8>     ~ -- ZenohID of the sender of the INIT message
/// +---------------+
/// ~      <u8>     ~ if Flag(A)==1 -- Cookie
/// +---------------+
/// ~   [InitExts]  ~ if Flag(Z)==1
/// +---------------+
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the INIT message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
///
/// (#) Max Sequence Number Bytes. It indicates the maximum number of bytes to be used when
///     encoding a sequence number as ZInt on the wire. The value encoded in (#) represents
///     the maximum number of bytes minus one. I.e., the actual advertised maximum number of
///     bytes is:
///
///         max_sn_bs := 1 + (sn_bs >> 2)
///    
///     Note that is valid for the largest sequence number to be smaller than the largest value
///     representable by the ZInt. The actual sequence number resolution (i.e. the amount
///     of sequence numbers available before wrapping over in a modulo operation) can be
///     computed as follows:
///
///         sn_res := 2^(7 * max_sn_bs) + 1
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
/// ```
///
#[derive(Debug, Clone, PartialEq)]
pub struct InitSyn {
    version: u8,
    pub whatami: WhatAmI,
    pub sn_bytes: SeqNumBytes,
    pub zid: ZenohId,
    pub exts: InitExts,
}

impl InitSyn {
    // Header flags
    // pub const FLAG_A: u8 = 1 << 5; // Reserved for InitAck
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(version: Version, whatami: WhatAmI, zid: ZenohId, sn_bytes: SeqNumBytes) -> Self {
        let mut msg = Self {
            version: version.stable,
            whatami,
            zid,
            sn_bytes,
            exts: InitExts::default(),
        };
        if let Some(exp) = version.experimental {
            let ext = InitExtExp::new(exp.get(), ZExtPolicy::Ignore);
            msg.exts.experimental = Some(ext);
        }
        msg
    }

    pub fn version(&self) -> Version {
        Version {
            stable: self.version,
            experimental: self
                .exts
                .experimental
                .as_ref()
                .and_then(|v| NonZeroZInt::new(v.value)),
        }
    }
}

impl ZMessage for InitSyn {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Init.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if has_exts {
            header |= InitSyn::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write(self.version));

        let whatami: u8 = match self.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let sn_bytes: u8 = (self.sn_bytes.value() - 1) & 0b111;
        zcheck!(wbuf.write((sn_bytes << 2) | whatami));

        zcheck!(wbuf.write_zenohid(&self.zid));

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<InitSyn> {
        if has_flag(header, InitAck::FLAG_A) {
            return None;
        }

        let version = zbuf.read()?;

        let tmp = zbuf.read()?;
        let whatami = match tmp & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return None,
        };
        let sn_bytes = SeqNumBytes::try_from(1 + ((tmp >> 2) & 0b111)).ok()?;

        let zid = zbuf.read_zenohid()?;

        let exts = if has_flag(header, InitSyn::FLAG_Z) {
            InitExts::read(zbuf)?
        } else {
            InitExts::default()
        };

        Some(InitSyn {
            version,
            whatami,
            sn_bytes,
            zid,
            exts,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitAck {
    pub version: u8,
    pub whatami: WhatAmI,
    pub sn_bytes: SeqNumBytes,
    pub zid: ZenohId,
    pub cookie: ZSlice,
    pub exts: InitExts,
}

impl InitAck {
    // Header flags
    pub const FLAG_A: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(
        version: Version,
        whatami: WhatAmI,
        zid: ZenohId,
        sn_bytes: SeqNumBytes,
        cookie: ZSlice,
    ) -> Self {
        let mut msg = Self {
            version: version.stable,
            whatami,
            sn_bytes,
            zid,
            cookie,
            exts: InitExts::default(),
        };
        if let Some(exp) = version.experimental {
            let ext = InitExtExp::new(exp.get(), ZExtPolicy::Ignore);
            msg.exts.experimental = Some(ext);
        }
        msg
    }
}

impl ZMessage for InitAck {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Init.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID | InitAck::FLAG_A;
        if has_exts {
            header |= InitAck::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write(self.version));

        let whatami: u8 = match self.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let sn_bytes: u8 = (self.sn_bytes.value() - 1) & 0b111;
        zcheck!(wbuf.write((sn_bytes << 2) | whatami));

        zcheck!(wbuf.write_zenohid(&self.zid));
        zcheck!(wbuf.write_zslice_array(self.cookie.clone()));

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<InitAck> {
        if !has_flag(header, InitAck::FLAG_A) {
            return None;
        }

        let version = zbuf.read()?;

        let tmp = zbuf.read()?;
        let whatami = match tmp & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return None,
        };
        let sn_bytes = SeqNumBytes::try_from(1 + ((tmp >> 2) & 0b111)).ok()?;

        let zid = zbuf.read_zenohid()?;
        let cookie = zbuf.read_zslice_array()?;

        let exts = if has_flag(header, InitAck::FLAG_Z) {
            InitExts::read(zbuf)?
        } else {
            InitExts::default()
        };

        Some(InitAck {
            version,
            whatami,
            sn_bytes,
            zid,
            cookie,
            exts,
        })
    }
}

/// # Init message extensions
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InitExtId {
    // 0x00: Reserved
    Experimental = 0x01,
    QoS = 0x02,
    Authentication = 0x03,
    // 0x04: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for InitExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const EXP: u8 = InitExtId::Experimental.id();
        const QOS: u8 = InitExtId::QoS.id();
        const AUT: u8 = InitExtId::Authentication.id();

        match id {
            EXP => Ok(InitExtId::Experimental),
            QOS => Ok(InitExtId::QoS),
            AUT => Ok(InitExtId::Authentication),
            _ => Err(()),
        }
    }
}

impl InitExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

type InitExtExp = ZExt<ZExtZInt<{ InitExtId::Experimental.id() }>>;
type InitExtQoS = ZExt<ZExtEmpty<{ InitExtId::QoS.id() }>>;
type InitExtAut = ZExt<ZExtProperties<{ InitExtId::Authentication.id() }>>;
type InitExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct InitExts {
    experimental: Option<InitExtExp>,
    pub qos: Option<InitExtQoS>,
    pub authentication: Option<InitExtAut>,
}

impl InitExts {
    fn is_empty(&self) -> bool {
        self.experimental.is_none() && self.qos.is_none() && self.authentication.is_none()
    }

    pub fn write(&self, wbuf: &mut WBuf) -> bool {
        if let Some(exp) = self.experimental.as_ref() {
            let has_more = self.qos.is_some() || self.authentication.is_some();
            zcheck!(exp.write(wbuf, has_more));
        }

        if let Some(qos) = self.qos.as_ref() {
            let has_more = self.authentication.is_some();
            zcheck!(qos.write(wbuf, has_more));
        }

        if let Some(aut) = self.authentication.as_ref() {
            let has_more = false;
            zcheck!(aut.write(wbuf, has_more));
        }

        true
    }

    pub fn read(zbuf: &mut ZBuf) -> Option<InitExts> {
        let mut exts = InitExts::default();

        loop {
            let header = zbuf.read()?;

            match InitExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    InitExtId::Experimental => {
                        let e: InitExtExp = ZExt::read(zbuf, header)?;
                        exts.experimental = Some(e);
                    }
                    InitExtId::QoS => {
                        let e: InitExtQoS = ZExt::read(zbuf, header)?;
                        exts.qos = Some(e);
                    }
                    InitExtId::Authentication => {
                        let e: InitExtAut = ZExt::read(zbuf, header)?;
                        exts.authentication = Some(e);
                    }
                },
                Err(_) => {
                    let _e: InitExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
