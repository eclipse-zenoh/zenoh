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
use super::TransportId;
use crate::net::protocol::core::ZInt;
use crate::net::protocol::io::{WBuf, ZBuf, ZSlice};
use crate::net::protocol::message::extension::{eid, has_more, ZExt, ZExtProperties, ZExtUnknown};
use crate::net::protocol::message::{has_flag, ZMessage};
use std::convert::TryFrom;
use std::time::Duration;

/// # Open message
///
/// After having succesfully complete the [`super::InitSyn`]-[`super::InitAck`] message exchange,
/// the OPEN message is sent on a link to finalize the initialization of the link and
/// associated transport with a zenoh node.
/// For convenience, we call [`OpenSyn`] and [`OpenAck`] an OPEN message with the A flag
/// is set to 0 and 1, respectively.
///
/// ```text
/// Flags:
/// - A: Ack            If A==0 then the message is an OpenSyn else it is an OpenAck
/// - T: Lease period   if T==1 then the lease period is in seconds else in milliseconds
/// - O: Option next    If O==1 then the next byte is an additional option
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|T|A|   OPEN  |
/// +-+-+-+---------+
/// %     lease     % -- Lease period of the sender of the OPEN message
/// +---------------+
/// %  initial_sn   % -- Initial SN proposed by the sender of the OPEN(*)
/// +---------------+
/// ~     <u8>      ~ if Flag(A)==0 (**) -- Cookie
/// +---------------+
/// ```
///
/// (*)     The initial sequence number MUST be compatible with the sequence number resolution agreed in the
///         [`super::InitSyn`]-[`super::InitAck`] message exchange
/// (**)    The cookie MUST be the same received in the [`super::InitAck`]from the corresponding zenoh node
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSyn {
    pub lease: Duration,
    pub initial_sn: ZInt,
    pub cookie: ZSlice,
    pub exts: OpenExts,
}

impl OpenSyn {
    // Header flags
    // pub const FLAG_A: u8 = 1 << 5; // Reserved for OpenAck
    pub const FLAG_T: u8 = 1 << 6;
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(lease: Duration, initial_sn: ZInt, cookie: ZSlice) -> Self {
        Self {
            lease,
            initial_sn,
            cookie,
            exts: OpenExts::default(),
        }
    }
}

impl ZMessage for OpenSyn {
    const ID: u8 = TransportId::Open.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID;
        let lease_secs = self.lease.as_millis() % 1_000 == 0;
        if lease_secs {
            header |= OpenSyn::FLAG_T;
        }
        if has_exts {
            header |= OpenSyn::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        if lease_secs {
            zcheck!(wbuf.write_zint(self.lease.as_secs() as ZInt));
        } else {
            zcheck!(wbuf.write_zint(self.lease.as_millis() as ZInt));
        }
        zcheck!(wbuf.write_zint(self.initial_sn));
        zcheck!(wbuf.write_zslice_array(self.cookie.clone()));

        // Write options
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<OpenSyn> {
        let lease = zbuf.read_zint()?;
        let lease = if has_flag(header, OpenSyn::FLAG_T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn = zbuf.read_zint()?;
        let cookie = zbuf.read_zslice_array()?;

        let exts = if has_flag(header, OpenSyn::FLAG_Z) {
            OpenExts::read(zbuf)?
        } else {
            OpenExts::default()
        };

        Some(OpenSyn {
            lease,
            initial_sn,
            cookie,
            exts,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAck {
    pub lease: Duration,
    pub initial_sn: ZInt,
    pub exts: OpenExts,
}

impl OpenAck {
    pub const FLAG_A: u8 = 1 << 5;
    pub const FLAG_T: u8 = 1 << 6;
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(lease: Duration, initial_sn: ZInt) -> Self {
        Self {
            lease,
            initial_sn,
            exts: OpenExts::default(),
        }
    }
}

impl ZMessage for OpenAck {
    const ID: u8 = TransportId::Open.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        // Compute extensions
        let has_exts = !self.exts.is_empty();

        // Build header
        let mut header = Self::ID | OpenAck::FLAG_A;
        let lease_secs = self.lease.as_millis() % 1_000 == 0;
        if lease_secs {
            header |= OpenAck::FLAG_T;
        }
        if has_exts {
            header |= OpenAck::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        if lease_secs {
            zcheck!(wbuf.write_zint(self.lease.as_secs() as ZInt));
        } else {
            zcheck!(wbuf.write_zint(self.lease.as_millis() as ZInt));
        }
        zcheck!(wbuf.write_zint(self.initial_sn));

        // Write options
        if has_exts {
            zcheck!(self.exts.write(wbuf));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<OpenAck> {
        if !has_flag(header, OpenAck::FLAG_A) {
            return None;
        }

        let lease = zbuf.read_zint()?;
        let lease = if has_flag(header, OpenAck::FLAG_T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn = zbuf.read_zint()?;

        let exts = if has_flag(header, OpenAck::FLAG_Z) {
            OpenExts::read(zbuf)?
        } else {
            OpenExts::default()
        };

        Some(OpenAck {
            lease,
            initial_sn,
            exts,
        })
    }
}

/// # Open message extensions
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OpenExtId {
    // 0x00: Reserved
    // 0x01: Reserved
    User = 0x02,
    // 0x03: Reserved
    Authentication = 0x04,
    // 0x05: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for OpenExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const USR: u8 = OpenExtId::User.id();
        const AUT: u8 = OpenExtId::Authentication.id();

        match id {
            USR => Ok(OpenExtId::User),
            AUT => Ok(OpenExtId::Authentication),
            _ => Err(()),
        }
    }
}

impl OpenExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}
type OpenExtUsr = ZExt<ZExtProperties<{ OpenExtId::User.id() }>>;
type OpenExtAut = ZExt<ZExtProperties<{ OpenExtId::Authentication.id() }>>;
type OpenExtUnk = ZExt<ZExtUnknown>;
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct OpenExts {
    pub authentication: Option<OpenExtAut>,
    pub user: Option<OpenExtUsr>,
}

impl OpenExts {
    fn is_empty(&self) -> bool {
        self.authentication.is_none() && self.user.is_none()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        if let Some(aut) = self.authentication.as_ref() {
            let has_more = self.user.is_some();
            zcheck!(aut.write(wbuf, has_more));
        }

        if let Some(usr) = self.user.as_ref() {
            zcheck!(usr.write(wbuf, false));
        }

        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<OpenExts> {
        let mut exts = OpenExts::default();

        loop {
            let header = zbuf.read()?;

            match OpenExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    OpenExtId::Authentication => {
                        let e: OpenExtAut = ZExt::read(zbuf, header)?;
                        exts.authentication = Some(e);
                    }
                    OpenExtId::User => {
                        let e: OpenExtUsr = ZExt::read(zbuf, header)?;
                        exts.user = Some(e);
                    }
                },
                Err(_) => {
                    let _e: OpenExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
