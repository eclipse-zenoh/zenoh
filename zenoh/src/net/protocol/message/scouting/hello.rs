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
use crate::net::protocol::core::{NonZeroZInt, Version, WhatAmI, ZenohId};
use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::message::extensions::{
    eid, has_more, ZExt, ZExtPolicy, ZExtProperties, ZExtUnknown, ZExtZInt,
};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;
use std::fmt;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

/// # Hello message
///
/// The [`Hello`] message is used to advertise the locators a zenoh node is reachable at.
/// The [`Hello`] message SHOULD be sent in a unicast fashion in response to a [`super::Scout`]
/// message as shown below:
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
/// Moreover, a [`Hello`] message MAY be sent in the network in a multicast
/// fashion to advertise the presence of zenoh node. The advertisement operation MAY be performed
/// periodically as shown below:
///
///```text
/// A                   B                   C
/// |       HELLO       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// |       HELLO       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// ```
///
/// Examples of locators included in the [`Hello`] message are:
///
/// ```text
///  udp/192.168.1.1:7447
///  tcp/192.168.1.1:7447
///  udp/224.0.0.224:7447
///  tcp/localhost:7447
/// ```
///
/// The [`Hello`] message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - L: Locators       If L==1 then the list of locators is present, else the src address is the locator
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|L|  HELLO  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |X|X|X|X|X|X|wai| (*)
/// +-+-+-+-+-+-+-+-+
/// ~     <u8>      ~ -- ZenohID
/// +---------------+
/// ~   <<utf8>>    ~ if Flag(L)==1 -- List of locators
/// +---------------+
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the HELLO message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
/// ```
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub locators: Vec<String>,
    pub exts: HelloExts,
}

impl Hello {
    // Header flags
    pub const FLAG_L: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(version: Version, whatami: WhatAmI, zid: ZenohId, locators: Vec<String>) -> Self {
        let mut msg = Self {
            version: version.stable,
            whatami,
            zid,
            locators,
            exts: HelloExts::default(),
        };
        if let Some(exp) = version.experimental {
            let ext = HelloExtExp::new(exp.get(), ZExtPolicy::Ignore);
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

impl ZMessage for Hello {
    type Proto = ScoutingProto;
    const ID: u8 = ScoutingId::Hello.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if !self.locators.is_empty() {
            header |= Hello::FLAG_L;
        }
        if has_exts {
            header |= Hello::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write(self.version));

        let wai: u8 = match self.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        zcheck!(wbuf.write(wai));

        zcheck!(wbuf.write_zenohid(&self.zid));

        if !self.locators.is_empty() {
            zcheck!(wbuf.write_string_array(self.locators.as_slice()));
        }

        // Write extensions
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Hello> {
        let version = zbuf.read()?;

        let tmp = zbuf.read()?;
        let whatami = match tmp & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return None,
        };

        let zid = zbuf.read_zenohid()?;

        let locators = if has_flag(header, Hello::FLAG_L) {
            zbuf.read_string_array()?
        } else {
            vec![]
        };

        let exts = if has_flag(header, Hello::FLAG_Z) {
            HelloExts::read(zbuf)?
        } else {
            HelloExts::default()
        };

        let msg = Hello {
            version,
            whatami,
            zid,
            locators,
            exts,
        };
        Some(msg)
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HelloExtId {
    // 0x00: Reserved
    Experimental = 0x01,
    User = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for HelloExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const EXP: u8 = HelloExtId::Experimental.id();
        const USR: u8 = HelloExtId::User.id();

        match id {
            EXP => Ok(HelloExtId::Experimental),
            USR => Ok(HelloExtId::User),
            _ => Err(()),
        }
    }
}

impl HelloExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

type HelloExtExp = ZExt<ZExtZInt<{ HelloExtId::Experimental.id() }>>;
type HelloExtUsr = ZExt<ZExtProperties<{ HelloExtId::User.id() }>>;
type HelloExtUnk = ZExt<ZExtUnknown>;

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct HelloExts {
    experimental: Option<HelloExtExp>,
    pub user: Option<HelloExtUsr>,
}

impl HelloExts {
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

    fn read(zbuf: &mut ZBuf) -> Option<HelloExts> {
        let mut exts = HelloExts::default();

        loop {
            let header = zbuf.read()?;

            match HelloExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    HelloExtId::Experimental => {
                        let e: HelloExtExp = ZExt::read(zbuf, header)?;
                        exts.experimental = Some(e);
                    }
                    HelloExtId::User => {
                        let e: HelloExtUsr = ZExt::read(zbuf, header)?;
                        exts.user = Some(e);
                    }
                },
                Err(_) => {
                    let _e: HelloExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = f.debug_struct("Hello");
        s.field("version", &self.version);
        s.field("whatami", &self.whatami);
        s.field("zid", &self.zid);
        s.field("locators", &self.locators);
        if let Some(exp) = self.exts.experimental.as_ref() {
            s.field("experimental", exp);
        }
        if let Some(usr) = self.exts.user.as_ref() {
            s.field("user", usr);
        }
        s.finish()
    }
}
