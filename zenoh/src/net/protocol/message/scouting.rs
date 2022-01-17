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
use super::{WireProperties, ZOpt};
use crate::net::link::Locator;
use std::fmt;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

// One trillion iterations:
// One 2164.382053981s 5965369712512
// Two 791.343083805s 5965369712512

// fn zint_len(mut v: usize) -> usize {
//     let mut n = 1;
//     while v > 0x7F {
//         v >>= 7;
//         n += 1;
//     }
//     n
// }

fn zint_len(v: ZInt) -> usize {
    const MASK_1: ZInt = ZInt::MAX << 7;
    const MASK_2: ZInt = ZInt::MAX << (7 * 2);
    const MASK_3: ZInt = ZInt::MAX << (7 * 3);
    const MASK_4: ZInt = ZInt::MAX << (7 * 4);
    const MASK_5: ZInt = ZInt::MAX << (7 * 5);
    const MASK_6: ZInt = ZInt::MAX << (7 * 6);
    const MASK_7: ZInt = ZInt::MAX << (7 * 7);
    const MASK_8: ZInt = ZInt::MAX << (7 * 8);
    const MASK_9: ZInt = ZInt::MAX << (7 * 9);

    if (v & MASK_1) == 0 {
        1
    } else if (v & MASK_2) == 0 {
        2
    } else if (v & MASK_3) == 0 {
        3
    } else if (v & MASK_4) == 0 {
        4
    } else if (v & MASK_5) == 0 {
        5
    } else if (v & MASK_6) == 0 {
        6
    } else if (v & MASK_7) == 0 {
        7
    } else if (v & MASK_8) == 0 {
        8
    } else if (v & MASK_9) == 0 {
        9
    } else {
        10
    }
}

pub mod id {
    // 0x00: Reserved

    // Scouting Messages
    pub const SCOUT: u8 = 0x01;
    pub const HELLO: u8 = 0x02;

    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

pub mod opt {
    // 0x00: Reserved

    // Scouting options
    pub const EXPERIMENTAL: u8 = 0x01;
    pub const ZENOH: u8 = 0x02;
    pub const USER: u8 = 0x03;

    // 0x04: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

/// # Experimental option
///
/// It indicates the experimental version of zenoh.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| EXP_VER |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~    version    ~
/// +---------------+
///
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ZOptExp {
    pub version: ZInt,
    pub len: usize,
}

impl ZOptExp {
    fn new(version: ZInt) -> Self {
        Self {
            version,
            len: zint_len(version),
        }
    }

    pub fn read_body(zbuf: &mut ZBuf, _header: u8) -> Option<Self> {
        let start = zbuf.readable();
        let version = zbuf.read_zint()?;
        let len = start - zbuf.readable();
        Some(Self { version, len })
    }
}

impl ZOpt for ZOptExp {
    fn header(&self) -> u8 {
        opt::EXPERIMENTAL
    }

    fn length(&self) -> usize {
        self.len
    }

    fn write_body(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_zint(self.version)
    }
}

/// # Routing ID
///
/// It includes the zenoh properties.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| RID     |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~      rid      ~
/// +---------------+
///

/// # Zenoh option
///
/// It includes the zenoh properties.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| Z_PROPS |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~  <property>   ~
/// +---------------+
///
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ZOptZenoh {
    pub inner: WireProperties,
}

impl ZOptZenoh {
    pub fn new() -> Self {
        Self {
            inner: WireProperties::new(),
        }
    }

    pub fn read_body(zbuf: &mut ZBuf, _header: u8) -> Option<Self> {
        let inner = zbuf.read_wire_properties()?;
        Some(Self { inner })
    }
}

impl Default for ZOptZenoh {
    fn default() -> Self {
        Self::new()
    }
}

impl ZOpt for ZOptZenoh {
    fn header(&self) -> u8 {
        opt::ZENOH
    }

    fn length(&self) -> usize {
        let len = zint_len(self.inner.len() as ZInt);
        self.inner
            .iter()
            .fold(len, |len, (k, v)| len + zint_len(*k) + v.len())
    }

    fn write_body(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_wire_properties(&self.inner)
    }
}

/// # User option
///
/// It includes the zenoh properties.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| U_PROPS |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~  <property>   ~
/// +---------------+
///
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ZOptUser {
    pub inner: WireProperties,
}

impl ZOptUser {
    pub fn new() -> Self {
        Self {
            inner: WireProperties::new(),
        }
    }

    pub fn read_body(zbuf: &mut ZBuf, _header: u8) -> Option<Self> {
        let inner = zbuf.read_wire_properties()?;
        Some(Self { inner })
    }
}

impl Default for ZOptUser {
    fn default() -> Self {
        Self::new()
    }
}

impl ZOpt for ZOptUser {
    fn header(&self) -> u8 {
        opt::USER
    }

    fn length(&self) -> usize {
        let len = zint_len(self.inner.len() as ZInt);
        self.inner
            .iter()
            .fold(len, |len, (k, v)| len + zint_len(*k) + v.len())
    }

    fn write_body(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_wire_properties(&self.inner)
    }
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
/// |    version    |
/// +---------------+
/// |X|X|X|X|X| what| (*)
/// +---+-------+---+
/// ~   zenoh_id    ~ if Flag(I)==1 -- ZenohID
/// +---------------+
///
/// (*) What. It indicates a bitmap of WhatAmI interests.
///    The valid bitflags are:
///    - 0b001: Router
///    - 0b010: Peer
///    - 0b100: Client
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| Z_PROPS |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~  scout_props  ~
/// +---------------+
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X| U_PROPS |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~  user_props   ~
/// +---------------+
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub version: Version,
    pub what: WhatAmIMatcher,
    pub zid: Option<ZenohId>,
    pub z_ps: ZOptZenoh,
    pub u_ps: ZOptUser,
}

impl Scout {
    // Header flags
    pub const FLAG_I: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_O: u8 = 1 << 7;

    fn opt_len(&self) -> usize {
        let mut opts = 0;
        if self.version.experimental.is_some() {
            opts += 1;
        }
        if !self.z_ps.inner.is_empty() {
            opts += 1;
        }
        if !self.u_ps.inner.is_empty() {
            opts += 1;
        }
        opts
    }
}

impl WBuf {
    fn write_scout_opts(&mut self, scout: &Scout, mut opts: usize) -> bool {
        if let Some(exp) = scout.version.experimental {
            opts -= 1;
            let exp = ZOptExp::new(exp.get());
            zcheck!(self.write_option(&exp, opts != 0));
        }

        if !scout.z_ps.inner.is_empty() {
            opts -= 1;
            zcheck!(self.write_option(&scout.z_ps, opts != 0));
        }

        if !scout.u_ps.inner.is_empty() {
            opts -= 1;
            zcheck!(self.write_option(&scout.u_ps, opts != 0));
        }

        opts == 0
    }

    pub fn write_scout(&mut self, scout: &Scout) -> bool {
        // Compute options
        let opts = scout.opt_len();

        // Build header
        let mut header = id::SCOUT;
        if scout.zid.is_some() {
            header |= Scout::FLAG_I;
        }
        if opts != 0 {
            header |= Scout::FLAG_O;
        }

        // Write header
        zcheck!(self.write(header));

        // Write body
        zcheck!(self.write(scout.version.stable));

        let what: u8 = scout.what.into();
        zcheck!(self.write(what));

        if let Some(zid) = scout.zid.as_ref() {
            zcheck!(self.write_zenohid(zid));
        }

        // Write options
        if opts != 0 {
            zcheck!(self.write_scout_opts(scout, opts));
        }

        true
    }
}

impl ZBuf {
    fn read_scout_opts(&mut self, scout: &mut Scout) -> Option<()> {
        loop {
            let opt = self.read()?;
            let len = self.read_zint_as_usize()?;

            match super::mid(opt) {
                opt::EXPERIMENTAL => {
                    let exp = ZOptExp::read_body(self, opt)?;
                    scout.version.experimental = NonZeroZInt::new(exp.version);
                }
                opt::ZENOH => scout.z_ps = ZOptZenoh::read_body(self, opt)?,
                opt::USER => scout.u_ps = ZOptUser::read_body(self, opt)?,
                _ => {
                    if !self.skip_bytes(len) {
                        return None;
                    }
                }
            }

            if !super::has_flag(opt, Scout::FLAG_O) {
                break;
            }
        }

        Some(())
    }

    pub fn read_scout(&mut self, header: u8) -> Option<Scout> {
        let version = Version {
            stable: self.read()?,
            experimental: None,
        };

        let tmp = self.read()?;
        let what = WhatAmIMatcher::try_from(tmp & 0b111)?;

        let zid = if super::has_flag(header, Scout::FLAG_I) {
            Some(self.read_zenohid()?)
        } else {
            None
        };

        let mut msg = Scout {
            version,
            what,
            zid,
            z_ps: ZOptZenoh::new(),
            u_ps: ZOptUser::new(),
        };

        if super::has_flag(header, Scout::FLAG_O) {
            self.read_scout_opts(&mut msg)?;
        }

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
/// |    version    |
/// +---------------+
/// |X|X|X|X|X|X|wai| (*)
/// +---+---+-------+
/// ~   zenoh_id    ~ -- ZenohID
/// +---------------+
/// ~   <locator>   ~ if Flag(L)==1 -- List of locators
/// +---------------+
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the HELLO message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub version: Version,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub locators: Vec<Locator>,
    pub z_ps: ZOptZenoh,
    pub u_ps: ZOptUser,
}

impl Hello {
    // Header flags
    pub const FLAG_L: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_O: u8 = 1 << 7;

    fn opt_len(&self) -> usize {
        let mut opts = 0;
        if self.version.experimental.is_some() {
            opts += 1;
        }
        if !self.z_ps.inner.is_empty() {
            opts += 1;
        }
        if !self.u_ps.inner.is_empty() {
            opts += 1;
        }
        opts
    }
}

impl WBuf {
    fn write_hello_opts(&mut self, hello: &Hello, mut opts: usize) -> bool {
        if let Some(exp) = hello.version.experimental {
            opts -= 1;
            let exp = ZOptExp::new(exp.get());
            zcheck!(self.write_option(&exp, opts != 0));
        }

        if !hello.z_ps.inner.is_empty() {
            opts -= 1;
            zcheck!(self.write_option(&hello.z_ps, opts != 0));
        }

        if !hello.u_ps.inner.is_empty() {
            opts -= 1;
            zcheck!(self.write_option(&hello.u_ps, opts != 0));
        }

        opts == 0
    }

    pub fn write_hello(&mut self, hello: &Hello) -> bool {
        // Compute options
        let opts = hello.opt_len();

        // Build header
        let mut header = id::HELLO;
        if !hello.locators.is_empty() {
            header |= Hello::FLAG_L;
        }
        if opts != 0 {
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
        zcheck!(self.write(wai));

        zcheck!(self.write_zenohid(&hello.zid));

        if !hello.locators.is_empty() {
            zcheck!(self.write_locators(hello.locators.as_slice()));
        }

        // Write options
        if opts != 0 {
            zcheck!(self.write_hello_opts(hello, opts));
        }

        true
    }
}

impl ZBuf {
    fn read_hello_opts(&mut self, hello: &mut Hello) -> Option<()> {
        loop {
            let opt = self.read()?;
            let len = self.read_zint_as_usize()?;

            match super::mid(opt) {
                opt::EXPERIMENTAL => {
                    let exp = ZOptExp::read_body(self, opt)?;
                    hello.version.experimental = NonZeroZInt::new(exp.version);
                }
                opt::ZENOH => hello.z_ps = ZOptZenoh::read_body(self, opt)?,
                opt::USER => hello.u_ps = ZOptUser::read_body(self, opt)?,
                _ => {
                    if !self.skip_bytes(len) {
                        return None;
                    }
                }
            }

            if !super::has_flag(opt, Scout::FLAG_O) {
                break;
            }
        }

        Some(())
    }

    pub fn read_hello(&mut self, header: u8) -> Option<Hello> {
        let version = Version {
            stable: self.read()?,
            experimental: None,
        };

        let tmp = self.read()?;
        let whatami = match tmp & 0b11 {
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

        let mut msg = Hello {
            version,
            whatami,
            zid,
            locators,
            z_ps: ZOptZenoh::new(),
            u_ps: ZOptUser::new(),
        };

        if super::has_flag(header, Scout::FLAG_O) {
            self.read_hello_opts(&mut msg)?;
        }

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
        z_ps: WireProperties,
        u_ps: WireProperties,
    ) -> ScoutingMessage {
        ScoutingMessage {
            body: ScoutingBody::Scout(Scout {
                version,
                what,
                zid,
                z_ps: ZOptZenoh { inner: z_ps },
                u_ps: ZOptUser { inner: u_ps },
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
        z_ps: WireProperties,
        u_ps: WireProperties,
    ) -> ScoutingMessage {
        ScoutingMessage {
            body: ScoutingBody::Hello(Hello {
                version,
                whatami,
                zid,
                locators,
                z_ps: ZOptZenoh { inner: z_ps },
                u_ps: ZOptUser { inner: u_ps },
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
