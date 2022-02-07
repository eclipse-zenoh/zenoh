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
use crate::net::protocol::core::{
    ConduitSn, ConduitSnList, NonZeroZInt, Priority, SeqNumBytes, Version, WhatAmI, ZInt, ZenohId,
};
use crate::net::protocol::io::{zint_len, WBuf, ZBuf};
use crate::net::protocol::message::extensions::{
    eid, has_more, ZExt, ZExtPolicy, ZExtProperties, ZExtUnknown, ZExtZInt, ZExtension,
};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;
use std::time::Duration;

impl ConduitSn {
    pub fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_zint(self.reliable) && wbuf.write_zint(self.best_effort)
    }

    pub fn read(zbuf: &mut ZBuf) -> Option<ConduitSn> {
        Some(ConduitSn {
            reliable: zbuf.read_zint()?,
            best_effort: zbuf.read_zint()?,
        })
    }
}

/// # Join message
///
/// The [`Join`] message is sent periodically on a multicast locator to advertise
/// the multicast transport parameters as shown below:
///
///```text
/// A                   B                   C
/// |       JOIN        |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// |       JOIN        |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// ```
///
/// The [`Join`] message structure is defined as follows:
///
///```text
/// Flags:
/// - X: Reserved
/// - T: Lease period   If T==1 then the lease period is in seconds else in milliseconds
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|T|X|   JOIN  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |X|X|X|sn_bs|wai| (#)(*)
/// +-----+-----+---+
/// ~     <u8>      ~ -- ZenohID of the sender of the JOIN message
/// +---------------+
/// %     lease     % -- Lease period of the sender of the JOIN message
/// +---------------+
/// ~    next_sn    ~ -- Next conduit sequence number
/// +---------------+
/// ~   [JoinExts]  ~ if Flag(Z)==1
/// +---------------+
/// ```
///
/// (#)(*) See [`super::InitSyn`]
///
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    version: u8,
    pub whatami: WhatAmI,
    pub sn_bytes: SeqNumBytes,
    pub zid: ZenohId,
    pub lease: Duration,
    next_sns: ConduitSn,
    pub exts: JoinExts,
}

impl Join {
    // Header flags
    // pub const FLAG_X: u8 = 1 << 5; // Reserved for future use
    pub const FLAG_T: u8 = 1 << 6;
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(
        version: Version,
        whatami: WhatAmI,
        zid: ZenohId,
        sn_bytes: SeqNumBytes,
        lease: Duration,
        next_sns: ConduitSn,
    ) -> Self {
        let mut msg = Self {
            version: version.stable,
            whatami,
            sn_bytes,
            zid,
            lease,
            next_sns,
            exts: JoinExts::default(),
        };
        if let Some(exp) = version.experimental {
            let ext = JoinExtExp::new(exp.get(), ZExtPolicy::Ignore);
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

    pub fn next_sns(&self) -> ConduitSnList {
        match self.exts.qos.as_ref() {
            Some(qos) => ConduitSnList::QoS(qos.array),
            None => ConduitSnList::Plain(self.next_sns),
        }
    }
}

impl ZMessage for Join {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Join.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        let lease_secs = self.lease.as_millis() % 1_000 == 0;
        if lease_secs {
            header |= Join::FLAG_T;
        }
        if has_exts {
            header |= Join::FLAG_Z;
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

        if lease_secs {
            zcheck!(wbuf.write_zint(self.lease.as_secs() as ZInt));
        } else {
            zcheck!(wbuf.write_zint(self.lease.as_millis() as ZInt));
        }

        zcheck!(self.next_sns.write(wbuf));

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Join> {
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

        let lease = zbuf.read_zint()?;
        let lease = if has_flag(header, Join::FLAG_T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };

        let next_sns = ConduitSn::read(zbuf)?;

        let exts = if has_flag(header, Join::FLAG_Z) {
            JoinExts::read(zbuf)?
        } else {
            JoinExts::default()
        };

        Some(Join {
            version,
            whatami,
            sn_bytes,
            zid,
            lease,
            next_sns,
            exts,
        })
    }
}

/// # Join message extensions
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum JoinExtId {
    // 0x00: Reserved
    Experimental = 0x01,
    QoS = 0x02,
    Authentication = 0x03,
    // 0x04: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for JoinExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const EXP: u8 = JoinExtId::Experimental.id();
        const QOS: u8 = JoinExtId::QoS.id();
        const AUT: u8 = JoinExtId::Authentication.id();

        match id {
            EXP => Ok(JoinExtId::Experimental),
            QOS => Ok(JoinExtId::QoS),
            AUT => Ok(JoinExtId::Authentication),
            _ => Err(()),
        }
    }
}

impl JoinExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

/// # QoS extension
///
/// It is an extension containing an array of Conduit SN.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  [ConduitSn]  ~
/// +---------------+
///
#[derive(Clone, PartialEq, Debug)]
pub struct ZExtConduitSN<const ID: u8, const NUM: usize> {
    pub array: [ConduitSn; NUM],
}

impl<const ID: u8, const NUM: usize> ZExtConduitSN<{ ID }, { NUM }> {
    pub fn new(array: [ConduitSn; NUM]) -> Self {
        Self { array }
    }
}

impl<const ID: u8, const NUM: usize> ZExtension for ZExtConduitSN<{ ID }, { NUM }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        self.array.iter().fold(0, |len, v| {
            len + zint_len(v.reliable) + zint_len(v.best_effort)
        })
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        for i in 0..NUM {
            zcheck!(self.array[i].write(wbuf));
        }
        true
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let mut array = [ConduitSn::default(); NUM];
        for i in array.iter_mut().take(NUM) {
            *i = ConduitSn::read(zbuf)?;
        }
        Some(Self { array })
    }
}

impl<const ID: u8, const NUM: usize> From<[ConduitSn; NUM]> for ZExtConduitSN<{ ID }, { NUM }> {
    fn from(array: [ConduitSn; NUM]) -> Self {
        Self::new(array)
    }
}

type JoinExtExp = ZExt<ZExtZInt<{ JoinExtId::Experimental.id() }>>;
type JoinExtQoS = ZExt<ZExtConduitSN<{ JoinExtId::QoS.id() }, { Priority::NUM }>>;
type JoinExtAut = ZExt<ZExtProperties<{ JoinExtId::Authentication.id() }>>;
type JoinExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, PartialEq)]
pub struct JoinExts {
    experimental: Option<JoinExtExp>,
    pub qos: Option<JoinExtQoS>,
    pub authentication: Option<JoinExtAut>,
}

impl JoinExts {
    fn is_empty(&self) -> bool {
        self.experimental.is_none() && self.qos.is_none() && self.authentication.is_none()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
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

    fn read(zbuf: &mut ZBuf) -> Option<JoinExts> {
        let mut exts = JoinExts::default();

        loop {
            let header = zbuf.read()?;

            match JoinExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    JoinExtId::Experimental => {
                        let e: JoinExtExp = ZExt::read(zbuf, header)?;
                        exts.experimental = Some(e);
                    }
                    JoinExtId::QoS => {
                        let e: JoinExtQoS = ZExt::read(zbuf, header)?;
                        exts.qos = Some(e);
                    }
                    JoinExtId::Authentication => {
                        let e: JoinExtAut = ZExt::read(zbuf, header)?;
                        exts.authentication = Some(e);
                    }
                },
                Err(_) => {
                    let _e: JoinExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
