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
use super::extensions::{ZExperimental, ZUnknown, ZUser};
use super::io::{WBuf, ZBuf};
use super::{zext, ZExt, ZExtPolicy};
use std::convert::TryFrom;
use std::fmt;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScoutingId {
    // 0x00: Reserved
    Scout = 0x01,
    Hello = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for ScoutingId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const SCT: u8 = ScoutingId::Scout.id();
        const HEL: u8 = ScoutingId::Hello.id();

        match id {
            SCT => Ok(ScoutingId::Scout),
            HEL => Ok(ScoutingId::Hello),
            _ => Err(()),
        }
    }
}

impl ScoutingId {
    const fn id(self) -> u8 {
        self as u8
    }
}

/// # Scouting protocol
///
/// In zenoh, scouting is used to discover any other zenoh node in the surroundings when
/// operating in a LAN-like environment. This is achieved by exploiting multicast capabilities
/// of the underlying network.
///
/// In case of operating on a multicast UDP/IP network, the scouting works as follows:
/// - A zenoh node sends out a SCOUT message transported by a UDP datagram addressed to a given
///   IP multicast group that reach any other zenoh node participating in the same group.
/// - Upon receiving the SCOUT message, zenoh nodes reply back with an HELLO message carried by
///   a second UDP datagram containing the information (i.e., the set of locators) that can be
///   used to reach them (e.g., the TCP/IP address).
/// - The zenoh node who initially sent the SCOUT message may use the information on the locators
///   contained on the received HELLO messages to initiate a zenoh session with the discovered
///   zenoh nodes.
///
/// It's worth remarking that scouting in zenoh is related to discovering where other zenoh nodes
/// are (e.g. peers and/or routers) and not to the discovery of the resources actually being
/// published or subscribed. In other words, the scouting mechanism in zenoh helps at answering
/// the following questions:
/// - What are the zenoh nodes that are around in the same LAN-like network?
/// - What address I can use to reach them?
///
/// Finally, it is worth highlighting that in the case of operating o a network that does not
/// support multicast communication, scouting could be achieved through a different mechanism
/// like DNS, SDP, etc.
///
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
        let mid = super::mid(header);
        let body = match ScoutingId::try_from(mid) {
            Ok(id) => match id {
                ScoutingId::Scout => ScoutingBody::Scout(self.read_scout(header)?),
                ScoutingId::Hello => ScoutingBody::Hello(self.read_hello(header)?),
            },
            Err(_) => {
                log::trace!("Scouting message with unknown ID: {}", mid);
                return None;
            }
        };

        #[cfg(feature = "stats")]
        let stop_readable = self.readable();

        Some(ScoutingMessage {
            body,
            #[cfg(feature = "stats")]
            size: NonZeroUsize::new(start_readable - stop_readable),
        })
    }
}

impl<T: Into<ScoutingBody>> From<T> for ScoutingMessage {
    fn from(msg: T) -> Self {
        Self {
            body: msg.into(),
            #[cfg(feature = "stats")]
            size: None,
        }
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
            let ext = ZExt::new(ZExperimental::new(exp.get()), ZExtPolicy::Ignore);
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
                .and_then(|v| NonZeroZInt::new(v.version)),
        }
    }
}

impl From<Scout> for ScoutingBody {
    fn from(msg: Scout) -> Self {
        Self::Scout(msg)
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

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct ScoutExts {
    experimental: Option<ZExt<ZExperimental<{ ScoutExtId::Experimental.id() }>>>,
    pub user: Option<ZExt<ZUser<{ ScoutExtId::User.id() }>>>,
}

impl ScoutExts {
    fn is_empty(&self) -> bool {
        self.experimental.is_none() && self.user.is_none()
    }
}

impl WBuf {
    fn write_scout_exts(&mut self, exts: &ScoutExts) -> bool {
        if let Some(exp) = exts.experimental.as_ref() {
            zcheck!(self.write_extension(exp, exts.user.is_some()));
        }

        if let Some(usr) = exts.user.as_ref() {
            zcheck!(self.write_extension(usr, false));
        }

        true
    }

    pub fn write_scout(&mut self, scout: &Scout) -> bool {
        // Compute options
        let has_exts = !scout.exts.is_empty();

        // Build header
        let mut header = ScoutingId::Scout.id();
        if scout.zid.is_some() {
            header |= Scout::FLAG_I;
        }
        if has_exts {
            header |= Scout::FLAG_Z;
        }

        // Write header
        zcheck!(self.write(header));

        // Write body
        zcheck!(self.write(scout.version));

        let what: u8 = scout.what.into();
        zcheck!(self.write(what));

        if let Some(zid) = scout.zid.as_ref() {
            zcheck!(self.write_zenohid(zid));
        }

        // Write options
        if has_exts {
            zcheck!(self.write_scout_exts(&scout.exts));
        }

        true
    }
}

impl ZBuf {
    fn read_scout_exts(&mut self) -> Option<ScoutExts> {
        let mut exts = ScoutExts::default();

        loop {
            let header = self.read()?;

            match ScoutExtId::try_from(super::mid(header)) {
                Ok(id) => match id {
                    ScoutExtId::Experimental => {
                        exts.experimental = Some(self.read_extension(header)?);
                    }
                    ScoutExtId::User => {
                        exts.user = Some(self.read_extension(header)?);
                    }
                },
                Err(_) => {
                    let _ = ZUnknown::read(self, header)?;
                }
            }

            if !zext::has_more(header) {
                break;
            }
        }

        Some(exts)
    }

    pub fn read_scout(&mut self, header: u8) -> Option<Scout> {
        let version = self.read()?;

        let tmp = self.read()?;
        let what = WhatAmIMatcher::try_from(tmp & 0b111)?;

        let zid = if super::has_flag(header, Scout::FLAG_I) {
            Some(self.read_zenohid()?)
        } else {
            None
        };

        let exts = if super::has_flag(header, Scout::FLAG_Z) {
            self.read_scout_exts()?
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
            let ext = ZExt::new(ZExperimental::new(exp.get()), ZExtPolicy::Ignore);
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
                .and_then(|v| NonZeroZInt::new(v.version)),
        }
    }
}

impl From<Hello> for ScoutingBody {
    fn from(msg: Hello) -> Self {
        Self::Hello(msg)
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

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct HelloExts {
    experimental: Option<ZExt<ZExperimental<{ HelloExtId::Experimental.id() }>>>,
    pub user: Option<ZExt<ZUser<{ HelloExtId::User.id() }>>>,
}

impl HelloExts {
    fn is_empty(&self) -> bool {
        self.experimental.is_none() && self.user.is_none()
    }
}

impl WBuf {
    fn write_hello_exts(&mut self, exts: &HelloExts) -> bool {
        if let Some(exp) = exts.experimental.as_ref() {
            zcheck!(self.write_extension(exp, exts.user.is_some()));
        }

        if let Some(usr) = exts.user.as_ref() {
            zcheck!(self.write_extension(usr, false));
        }

        true
    }

    pub fn write_hello(&mut self, hello: &Hello) -> bool {
        // Compute options
        let has_exts = !hello.exts.is_empty();

        // Build header
        let mut header = ScoutingId::Hello.id();
        if !hello.locators.is_empty() {
            header |= Hello::FLAG_L;
        }
        if has_exts {
            header |= Hello::FLAG_Z;
        }

        // Write header
        zcheck!(self.write(header));

        // Write body
        zcheck!(self.write(hello.version));

        let wai: u8 = match hello.whatami {
            whatami::WhatAmI::Router => 0b00,
            whatami::WhatAmI::Peer => 0b01,
            whatami::WhatAmI::Client => 0b10,
        };
        zcheck!(self.write(wai));

        zcheck!(self.write_zenohid(&hello.zid));

        if !hello.locators.is_empty() {
            zcheck!(self.write_string_array(hello.locators.as_slice()));
        }

        // Write options
        if has_exts {
            zcheck!(self.write_hello_exts(&hello.exts));
        }

        true
    }
}

impl ZBuf {
    fn read_hello_exts(&mut self) -> Option<HelloExts> {
        let mut exts = HelloExts::default();

        loop {
            let header = self.read()?;

            match HelloExtId::try_from(super::mid(header)) {
                Ok(id) => match id {
                    HelloExtId::Experimental => {
                        exts.experimental = Some(self.read_extension(header)?);
                    }
                    HelloExtId::User => {
                        exts.user = Some(self.read_extension(header)?);
                    }
                },
                Err(_) => {
                    let _ = ZUnknown::read(self, header)?;
                }
            }

            if !zext::has_more(header) {
                break;
            }
        }

        Some(exts)
    }

    pub fn read_hello(&mut self, header: u8) -> Option<Hello> {
        let version = self.read()?;

        let tmp = self.read()?;
        let whatami = match tmp & 0b11 {
            0b00 => whatami::WhatAmI::Router,
            0b01 => whatami::WhatAmI::Peer,
            0b10 => whatami::WhatAmI::Client,
            _ => return None,
        };

        let zid = self.read_zenohid()?;

        let locators = if super::has_flag(header, Hello::FLAG_L) {
            self.read_string_array()?
        } else {
            vec![]
        };

        let exts = if super::has_flag(header, Hello::FLAG_Z) {
            self.read_hello_exts()?
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
