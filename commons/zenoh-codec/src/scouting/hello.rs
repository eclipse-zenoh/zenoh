//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use alloc::{vec, vec::Vec};

use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtUnknown, ZExtZBuf},
    core::{Locator, WhatAmI, ZenohIdProto},
    scouting::{
        hello::{
            ext::{self, GroupsType},
            flag, HelloProto,
        },
        id,
    },
};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};

impl<W> WCodec<&HelloProto, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &HelloProto) -> Self::Output {
        let HelloProto {
            version,
            whatami,
            zid,
            locators,
            ext_groups,
        } = x;

        // Header
        let mut header = id::HELLO;
        if !locators.is_empty() {
            header |= flag::L;
        }
        let n_exts = (ext_groups != &GroupsType::default()) as u8;
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, version)?;

        let mut flags: u8 = 0;
        let whatami: u8 = match whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        flags |= whatami & 0b11;
        flags |= ((zid.size() - 1) as u8) << 4;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(zid.size());
        lodec.write(&mut *writer, zid)?;

        if !locators.is_empty() {
            self.write(&mut *writer, locators.as_slice())?;
        }

        // Extensions
        if ext_groups != &GroupsType::default() {
            self.write(&mut *writer, (ext_groups, false))?;
        }

        Ok(())
    }
}

impl<R> RCodec<HelloProto, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<HelloProto, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<HelloProto, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<HelloProto, Self::Error> {
        if imsg::mid(self.header) != id::HELLO {
            return Err(DidntRead);
        }

        // Body
        let version: u8 = self.codec.read(&mut *reader)?;
        let flags: u8 = self.codec.read(&mut *reader)?;
        let whatami = match flags & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return Err(DidntRead),
        };
        let length = 1 + ((flags >> 4) as usize);
        let lodec = Zenoh080Length::new(length);
        let zid: ZenohIdProto = lodec.read(&mut *reader)?;

        let locators = if imsg::has_flag(self.header, flag::L) {
            let locs: Vec<Locator> = self.codec.read(&mut *reader)?;
            locs
        } else {
            vec![]
        };
        let mut ext_groups = GroupsType::default();

        // Extensions
        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::Groups::ID => {
                    let (b, ext): (ZExtZBuf<{ ext::Groups::ID }>, bool) =
                        eodec.read(&mut *reader)?;

                    let mut br = b.value.reader();
                    ext_groups = self.codec.read(&mut br)?;

                    has_ext = ext;
                }
                _ => {
                    let (_, more): (ZExtUnknown, bool) = self.codec.read(&mut *reader)?;
                    has_ext = more;
                }
            }
        }

        Ok(HelloProto {
            version,
            zid,
            whatami,
            locators,
            ext_groups,
        })
    }
}
