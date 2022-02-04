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
use super::{ScoutingId, ScoutingProto};
use crate::net::protocol::core::whatami::WhatAmIMatcher;
use crate::net::protocol::core::{NonZeroZInt, Version, ZenohId};
use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::message::extensions::{
    eid, has_more, ZExt, ZExtPolicy, ZExtProperties, ZExtUnknown, ZExtZInt,
};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;
use std::fmt;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

/// # Scout message
///
/// The [`Scout`] message MAY be sent at any point in time to discover the available zenoh nodes in the
/// network. The [`Scout`] message SHOULD be sent in a multicast or broadcast fashion. Upon receiving a
/// [`Scout`] message, a zenoh node MUST first verify whether the matching criteria are satisfied, then
/// it SHOULD reply with a [`super::Hello`] message in a unicast fashion including all the requested
/// information.
///
/// The scouting message flow is the following:
///
/// ```text
/// A                   B                   C
/// |       SCOUT       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// |       HELLO       |                   |
/// |<------------------|                   |
/// |                   |      HELLO        |
/// |<--------------------------------------|
/// |                   |                   |
/// ```
///
/// The SCOUT message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - I: ZenohID        If I==1 then the ZenohID of the scouter is present.
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|I|  SCOUT  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |X|X|X|X|X| what| (*)
/// +-+-+-+-+-+-+-+-+
/// ~     <u8>      ~ if Flag(I)==1 -- ZenohID
/// +---------------+
///
/// (*) What. It indicates a bitmap of WhatAmI interests.
///    The valid bitflags are:
///    - 0b001: Router
///    - 0b010: Peer
///    - 0b100: Client
/// ```
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    version: u8,
    pub what: WhatAmIMatcher,
    pub zid: Option<ZenohId>,
    pub exts: ScoutExts,
}

impl Scout {
    // Header flags
    pub const FLAG_I: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(version: Version, what: WhatAmIMatcher, zid: Option<ZenohId>) -> Self {
        let mut msg = Self {
            version: version.stable,
            what,
            zid,
            exts: ScoutExts::default(),
        };
        if let Some(exp) = version.experimental {
            let ext = ScoutExtExp::new(exp.get(), ZExtPolicy::Ignore);
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

impl ZMessage for Scout {
    type Proto = ScoutingProto;
    const ID: u8 = ScoutingId::Scout.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if self.zid.is_some() {
            header |= Scout::FLAG_I;
        }
        if has_exts {
            header |= Scout::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write(self.version));

        let what: u8 = self.what.into();
        zcheck!(wbuf.write(what));

        if let Some(zid) = self.zid.as_ref() {
            zcheck!(wbuf.write_zenohid(zid));
        }

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Scout> {
        let version = zbuf.read()?;

        let tmp = zbuf.read()?;
        let what = WhatAmIMatcher::try_from(tmp & 0b111)?;

        let zid = if has_flag(header, Scout::FLAG_I) {
            Some(zbuf.read_zenohid()?)
        } else {
            None
        };

        let exts = if has_flag(header, Scout::FLAG_Z) {
            ScoutExts::read(zbuf)?
        } else {
            ScoutExts::default()
        };

        let msg = Scout {
            version,
            what,
            zid,
            exts,
        };
        Some(msg)
    }
}

impl fmt::Display for Scout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = f.debug_struct("Scout");
        s.field("version", &self.version);
        s.field("what", &self.what);
        if let Some(zid) = self.zid.as_ref() {
            s.field("zid", zid);
        }
        if let Some(exp) = self.exts.experimental.as_ref() {
            s.field("experimental", exp);
        }
        if let Some(usr) = self.exts.user.as_ref() {
            s.field("user", usr);
        }
        s.finish()
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScoutExtId {
    // 0x00: Reserved
    Experimental = 0x01,
    User = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for ScoutExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const EXP: u8 = ScoutExtId::Experimental.id();
        const USR: u8 = ScoutExtId::User.id();

        match id {
            EXP => Ok(ScoutExtId::Experimental),
            USR => Ok(ScoutExtId::User),
            _ => Err(()),
        }
    }
}

impl ScoutExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

type ScoutExtExp = ZExt<ZExtZInt<{ ScoutExtId::Experimental.id() }>>;
type ScoutExtUsr = ZExt<ZExtProperties<{ ScoutExtId::User.id() }>>;
type ScoutExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct ScoutExts {
    experimental: Option<ScoutExtExp>,
    pub user: Option<ScoutExtUsr>,
}

impl ScoutExts {
    fn is_empty(&self) -> bool {
        self.experimental.is_none() && self.user.is_none()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        if let Some(exp) = self.experimental.as_ref() {
            zcheck!(exp.write(wbuf, self.user.is_some()));
        }

        if let Some(usr) = self.user.as_ref() {
            zcheck!(usr.write(wbuf, false));
        }

        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<ScoutExts> {
        let mut exts = ScoutExts::default();

        loop {
            let header = zbuf.read()?;

            match ScoutExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    ScoutExtId::Experimental => {
                        let e: ScoutExtExp = ZExt::read(zbuf, header)?;
                        exts.experimental = Some(e);
                    }
                    ScoutExtId::User => {
                        let e: ScoutExtUsr = ZExt::read(zbuf, header)?;
                        exts.user = Some(e);
                    }
                },
                Err(_) => {
                    let _e: ScoutExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
