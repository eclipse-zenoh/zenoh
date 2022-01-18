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
use super::core::whatami::WhatAmIMatcher;
use super::core::*;
use super::io::{WBuf, ZBuf};
use super::{WireProperties, ZOpts};
use crate::net::link::Locator;
use std::fmt;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

pub mod id {
    // 0x00: Reserved

    // Scouting Messages
    pub const SCOUT: u8 = 0x01;
    pub const HELLO: u8 = 0x02;

    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

/// # Scout message
///
/// The SCOUT message MAY be sent at any point in time to discover the available zenoh nodes in the
/// network. The SCOUT message SHOULD be sent in a multicast or broadcast fashion. Upon receiving a
/// SCOUT message, a zenoh node MUST first verify whether the matching criteria are satisfied, then
/// it SHOULD reply with a HELLO message in a unicast fashion including all the requested information.
///
/// The scouting message flow is the following:
///
/// ```text
///     A                   B                   C
///     |       SCOUT       |                   |
///     |------------------>|                   |
///     |         \---------------------------->|
///     |                   |                   |
///     |       HELLO       |                   |
///     |<------------------|                   |
///     |                   |      HELLO        |
///     |<--------------------------------------|
///     |                   |                   |
/// ```
///
/// The SCOUT message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - I: ZenohID        If I==1 then the ZenohID of the scouter is present.
/// - X: Reserved
/// - O: Option zero    If O==1 then the Opt0 is included.
///
/// Options:
/// - Byte 0 bits:
///  - 0 Z: Scout properties    If Z==1 then scout properties are included.
///  - 1 U: User properties     If U==1 then user properties are included.
///  - 2 E: Experimental        If E==1 then the zenoh version is considered to be experimental.
///  - 3 X: Reserved
///  - 4 \
///  - 5  : What(*)             The bitmap of WhatAmI interests.
///  - 6 /
///  - 7 O: Option one          If O==1 then the additional Opt1 is present. It always 0 for the time being.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|I|  SCOUT  |
/// +-+-+-+---------+
/// | vmaj  | vmin  |
/// +---------------+
/// |X|X|X| what|zis| (*)(#)
/// +---+---+-------+
/// ~   zenoh_id    ~ if Flag(I)==1 -- ZenohID
/// +---------------+
/// |O|X|X|X|X|E|U|Z| if Flag(O)==1 -- Opt0
/// +-+-+-+-+-+-+-+-+
/// ~  exp_version  ~ if Opt0(E)==1 -- ZInt
/// +---------------+
/// ~  scout_props  ~ if Opt0(Z)==1 -- Properties
/// +---------------+
/// ~ u_properties  ~ if Opt0(U)==1 -- Properties
/// +---------------+
///
/// (#) ZInt size. It indicates the supported ZInt size by the sender of the message.
///    The ZInt size represents the maximum size in memory of the ZInt and not the encoded
///    size on the wire. The valid ZInt size values in binary representation are:
///    - 0b00: 8 bits
///    - 0b01: 16 bits
///    - 0b10: 32 bits
///    - 0b11: 64 bits
///    That is, the valid ZInt size values are calculated as log_2(size in bits / 8).
///
/// (*) What. It indicates a bitmap of WhatAmI interests.
///    The valid bitflags are:
///    - 0b001: Router
///    - 0b010: Peer
///    - 0b100: Client
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub version: Version,
    pub zsize: ZIntSize,
    pub what: WhatAmIMatcher,
    pub zid: Option<ZenohId>,
    pub s_ps: WireProperties,
    pub u_ps: WireProperties,
}

impl Scout {
    // Header flags
    pub const FLAG_I: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_O: u8 = 1 << 7;

    // Opt0 flags
    pub const OPT0_Z: u8 = 1 << 0;
    pub const OPT0_U: u8 = 1 << 1;
    pub const OPT0_E: u8 = 1 << 2;
    // pub const OPT0_X: u8 = 1 << 3; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 4; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 5; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 6; // Reserved for future use
    // pub const OPT0_0: u8 = 1 << 7; // Reserved for future use
}

impl WBuf {
    pub fn write_scout(&mut self, scout: &Scout) -> bool {
        // Build options
        let mut opt = ZOpts::<1>::default();
        if !scout.s_ps.is_empty() {
            opt[0] |= Scout::OPT0_Z;
        }
        if !scout.u_ps.is_empty() {
            opt[0] |= Scout::OPT0_U;
        }
        if scout.version.experimental.is_some() {
            opt[0] |= Scout::OPT0_E;
        }

        // Build header
        let mut header = id::SCOUT;
        if scout.zid.is_some() {
            header |= Scout::FLAG_I;
        }
        if opt[0] != 0 {
            header |= Scout::FLAG_O;
        }

        // Write header
        zcheck!(self.write(header));

        // Write body
        zcheck!(self.write(scout.version.stable));

        let zis: u8 = match scout.zsize {
            ZIntSize::U8 => 0b00,
            ZIntSize::U16 => 0b01,
            ZIntSize::U32 => 0b10,
            ZIntSize::U64 => 0b11,
        };
        let what: u8 = scout.what.into();
        zcheck!(self.write(what << 2 | zis));

        if let Some(zid) = scout.zid.as_ref() {
            zcheck!(self.write_zenohid(zid));
        }

        // Write options
        if opt[0] != 0 {
            zcheck!(self.write(opt[0]));

            if let Some(exp) = scout.version.experimental {
                zcheck!(self.write_zint(exp.get()));
            }
            if !scout.s_ps.is_empty() {
                zcheck!(self.write_wire_properties(&scout.s_ps));
            }
            if !scout.u_ps.is_empty() {
                zcheck!(self.write_wire_properties(&scout.u_ps));
            }
        }

        true
    }
}

impl ZBuf {
    pub fn read_scout(&mut self, header: u8) -> Option<Scout> {
        let mut version = Version {
            stable: self.read()?,
            experimental: None,
        };

        let tmp = self.read()?;
        let zsize = match tmp & 0b11 {
            0b00 => ZIntSize::U8,
            0b01 => ZIntSize::U16,
            0b10 => ZIntSize::U32,
            0b11 => ZIntSize::U64,
            _ => return None,
        };
        let what = WhatAmIMatcher::try_from((tmp >> 2) & 0b111)?;

        let zid = if super::has_flag(header, Scout::FLAG_I) {
            Some(self.read_zenohid()?)
        } else {
            None
        };

        let mut s_ps = WireProperties::new();
        let mut u_ps = WireProperties::new();
        if super::has_flag(header, Scout::FLAG_O) {
            let opt0 = self.read()?;

            if super::has_flag(opt0, Scout::OPT0_E) {
                version.experimental = NonZeroZInt::new(self.read_zint()?);
            }
            if super::has_flag(opt0, Scout::OPT0_Z) {
                s_ps = self.read_wire_properties()?;
            }
            if super::has_flag(opt0, Scout::OPT0_U) {
                u_ps = self.read_wire_properties()?;
            }
        }

        let msg = Scout {
            version,
            zsize,
            what,
            zid,
            s_ps,
            u_ps,
        };
        Some(msg)
    }
}

/// # Hello message
///
/// The HELLO message is used to advertise the locators a zenoh node is reachable at.
/// The HELLO message SHOULD be sent in a unicast fashion in response to a SCOUT message as shown below:
///
/// ```text
///     A                   B                   C
///     |       SCOUT       |                   |
///     |------------------>|                   |
///     |         \---------------------------->|
///     |                   |                   |
///     |       HELLO       |                   |
///     |<------------------|                   |
///     |                   |      HELLO        |
///     |<--------------------------------------|
///     |                   |                   |
/// ```
///
/// Moreover, a HELLO message MAY be sent in the network in a multicast
/// fashion to advertise the presence of zenoh node. The advertisement operation MAY be performed
/// periodically as shown below:
///
///```text
///     A                   B                   C
///     |       HELLO       |                   |
///     |------------------>|                   |
///     |         \---------------------------->|
///     |                   |                   |
///    ...                 ...                 ...
///     |                   |                   |
///     |       HELLO       |                   |
///     |------------------>|                   |
///     |         \---------------------------->|
///     |                   |                   |
///    ...                 ...                ...
///     |                   |                   |
/// ```
///
/// Examples of locators included in the HELLO message are:
///
/// ```text
///  udp/192.168.1.1:7447
///  tcp/192.168.1.1:7447
///  udp/224.0.0.224:7447
///  tcp/localhost:7447
/// ```
///
/// The HELLO message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - L: Locators       If L==1 then the list of locators is present, else the src address is the locator
/// - X: Reserved
/// - O: Option zero    If O==1 then the Opt0 is included.
///
/// Options:
/// - Byte 0 bits:
///  - 0 Z: Hello properties
///  - 1 U: User properties
///  - 2 X: Reserved
///  - 3 X: Reserved
///  - 4 X: Reserved
///  - 5 X: Reserved
///  - 6 X: Reserved
///  - 7 O: Option one          If O==1 then the additional Opt1 is present. It always 0 for the time being.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|L|  HELLO  |
/// +-+-+-+---------+
/// | vmaj  | vmin  |
/// +---------------+
/// |X|X|X|X|wai|zis| (*)(#)
/// +---+---+-------+
/// ~   zenoh_id    ~ -- ZenohID
/// +---------------+
/// ~   <locator>   ~ if Flag(L)==1 -- List of locators
/// +---------------+
/// |O|X|X|X|X|E|U|Z| if Flag(O)==1 -- Opt0
/// +-+-+-+-+-+-+-+-+
/// ~  exp_version  ~ if Opt0(E)==1 -- ZInt
/// +---------------+
/// ~  hello_props  ~ if Opt0(Z)==1 -- Properties
/// +---------------+
/// ~ u_properties  ~ if Opt0(U)==1 -- Properties
/// +---------------+
///
/// (#) ZInt size. It indicates the supported ZInt size by the sender of the message.
///    The ZInt size represents the maximum size in memory of the ZInt and not the encoded
///    size on the wire. The valid ZInt size values in binary representation are:
///    - 0b00: 8 bits
///    - 0b01: 16 bits
///    - 0b10: 32 bits
///    - 0b11: 64 bits
///    That is, the valid ZInt size values are calculated as log_2(size in bits / 8).
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the HELLO message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub version: Version,
    pub zsize: ZIntSize,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub locators: Vec<Locator>,
    pub h_ps: WireProperties,
    pub u_ps: WireProperties,
}

impl Hello {
    // Header flags
    pub const FLAG_L: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_O: u8 = 1 << 7;

    // Opt0 flags
    pub const OPT0_Z: u8 = 1 << 0;
    pub const OPT0_U: u8 = 1 << 1;
    pub const OPT0_E: u8 = 1 << 2;
    // pub const OPT0_X: u8 = 1 << 3; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 4; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 5; // Reserved for future use
    // pub const OPT0_X: u8 = 1 << 6; // Reserved for future use
    // pub const OPT0_0: u8 = 1 << 7; // Reserved for future use
}

impl WBuf {
    pub fn write_hello(&mut self, hello: &Hello) -> bool {
        // Build options
        let mut opt = ZOpts::<1>::default();
        if !hello.h_ps.is_empty() {
            opt[0] |= Hello::OPT0_Z;
        }
        if !hello.u_ps.is_empty() {
            opt[0] |= Hello::OPT0_U;
        }
        if hello.version.experimental.is_some() {
            opt[0] |= Hello::OPT0_E;
        }

        // Build header
        let mut header = id::HELLO;
        if !hello.locators.is_empty() {
            header |= Hello::FLAG_L;
        }
        if opt[0] != 0 {
            header |= Hello::FLAG_O;
        }

        // Write header
        zcheck!(self.write(header));

        // Write body
        zcheck!(self.write(hello.version.stable));

        let wai: u8 = match hello.whatami {
            whatami::WhatAmI::Router => 0b00,
            whatami::WhatAmI::Peer => 0b01,
            whatami::WhatAmI::Client => 0b10,
        };
        let zis: u8 = match hello.zsize {
            ZIntSize::U8 => 0b00,
            ZIntSize::U16 => 0b01,
            ZIntSize::U32 => 0b10,
            ZIntSize::U64 => 0b11,
        };
        zcheck!(self.write(wai << 2 | zis));

        zcheck!(self.write_zenohid(&hello.zid));

        if !hello.locators.is_empty() {
            zcheck!(self.write_locators(hello.locators.as_slice()));
        }

        // Write options
        if opt[0] != 0 {
            zcheck!(self.write(opt[0]));

            if let Some(exp) = hello.version.experimental {
                zcheck!(self.write_zint(exp.get()));
            }
            if !hello.h_ps.is_empty() {
                zcheck!(self.write_wire_properties(&hello.h_ps));
            }
            if !hello.u_ps.is_empty() {
                zcheck!(self.write_wire_properties(&hello.u_ps));
            }
        }

        true
    }
}

impl ZBuf {
    pub fn read_hello(&mut self, header: u8) -> Option<Hello> {
        let mut version = Version {
            stable: self.read()?,
            experimental: None,
        };

        let tmp = self.read()?;

        let zsize = match tmp & 0b11 {
            0b00 => ZIntSize::U8,
            0b01 => ZIntSize::U16,
            0b10 => ZIntSize::U32,
            0b11 => ZIntSize::U64,
            _ => return None,
        };
        let whatami = match (tmp >> 2) & 0b11 {
            0b00 => whatami::WhatAmI::Router,
            0b01 => whatami::WhatAmI::Peer,
            0b10 => whatami::WhatAmI::Client,
            _ => return None,
        };

        let zid = self.read_zenohid()?;

        let locators = if super::has_flag(header, Hello::FLAG_L) {
            self.read_locators()?
        } else {
            vec![]
        };

        let mut h_ps = WireProperties::new();
        let mut u_ps = WireProperties::new();
        if super::has_flag(header, Hello::FLAG_O) {
            let opt0 = self.read()?;

            if super::has_flag(opt0, Hello::OPT0_E) {
                version.experimental = NonZeroZInt::new(self.read_zint()?);
            }
            if super::has_flag(opt0, Hello::OPT0_Z) {
                h_ps = self.read_wire_properties()?;
            }
            if super::has_flag(opt0, Hello::OPT0_U) {
                u_ps = self.read_wire_properties()?;
            }
        }

        let msg = Hello {
            version,
            zsize,
            whatami,
            zid,
            locators,
            h_ps,
            u_ps,
        };
        Some(msg)
    }
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let locators = self
            .locators
            .iter()
            .map(|locator| locator.to_string())
            .collect::<Vec<String>>();
        f.debug_struct("Hello")
            .field("pid", &self.zid)
            .field("whatami", &self.whatami)
            .field("locators", &locators)
            .finish()
    }
}

/// # Scouting message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoutingBody {
    Scout(Scout),
    Hello(Hello),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
    #[cfg(feature = "stats")]
    pub size: Option<NonZeroUsize>,
}

impl ScoutingMessage {
    pub fn make_scout(
        version: Version,
        what: WhatAmIMatcher,
        zid: Option<ZenohId>,
        s_ps: WireProperties,
        u_ps: WireProperties,
    ) -> ScoutingMessage {
        ScoutingMessage {
            body: ScoutingBody::Scout(Scout {
                version,
                zsize: ZIntSize::get(),
                what,
                zid,
                s_ps,
                u_ps,
            }),
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_hello(
        version: Version,
        whatami: WhatAmI,
        zid: ZenohId,
        locators: Vec<Locator>,
        h_ps: WireProperties,
        u_ps: WireProperties,
    ) -> ScoutingMessage {
        ScoutingMessage {
            body: ScoutingBody::Hello(Hello {
                version,
                zsize: ZIntSize::get(),
                whatami,
                zid,
                locators,
                h_ps,
                u_ps,
            }),
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}

impl WBuf {
    #[allow(clippy::let_and_return)] // Necessary when feature "stats" is disabled
    pub fn write_scouting_message(&mut self, msg: &mut ScoutingMessage) -> bool {
        #[cfg(feature = "stats")]
        let start_written = self.len();

        let res = match &mut msg.body {
            ScoutingBody::Scout(scout) => self.write_scout(scout),
            ScoutingBody::Hello(hello) => self.write_hello(hello),
        };

        #[cfg(feature = "stats")]
        {
            let stop_written = self.len();
            msg.size = NonZeroUsize::new(stop_written - start_written);
        }

        res
    }
}

impl ZBuf {
    pub fn read_scouting_message(&mut self) -> Option<ScoutingMessage> {
        #[cfg(feature = "stats")]
        let start_readable = self.readable();

        // Read the header
        let header = self.read()?;

        // Read the message
        let body = match super::mid(header) {
            id::SCOUT => ScoutingBody::Scout(self.read_scout(header)?),
            id::HELLO => ScoutingBody::Hello(self.read_hello(header)?),
            unknown => {
                log::trace!("Scouting message with unknown ID: {}", unknown);
                return None;
            }
        };

        #[cfg(feature = "stats")]
        let stop_readable = self.readable();

        Some(ScoutingMessage {
            body,
            #[cfg(feature = "stats")]
            size: std::num::NonZeroUsize::new(start_readable - stop_readable),
        })
    }
}
