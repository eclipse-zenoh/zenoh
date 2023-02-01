//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use alloc::{vec, vec::Vec};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{Locator, WhatAmI, ZenohId},
    scouting::Hello,
    transport::tmsg,
};

impl<W> WCodec<&Hello, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Hello) -> Self::Output {
        // Header
        let mut header = tmsg::id::HELLO;
        if !x.locators.is_empty() {
            header |= tmsg::flag::L;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.version)?;
        let whatami: u8 = match x.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        self.write(&mut *writer, whatami)?;
        self.write(&mut *writer, &x.zid)?;
        if !x.locators.is_empty() {
            self.write(&mut *writer, x.locators.as_slice())?;
        }

        Ok(())
    }
}

impl<R> RCodec<Hello, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        let codec = Zenoh080Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Hello, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        if imsg::mid(self.header) != imsg::id::HELLO {
            return Err(DidntRead);
        }

        let version: u8 = self.codec.read(&mut *reader)?;
        let flags: u8 = self.codec.read(&mut *reader)?;
        let whatami = match flags & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return Err(DidntRead),
        };

        let zid: ZenohId = self.codec.read(&mut *reader)?;
        let locators = if imsg::has_flag(self.header, tmsg::flag::L) {
            let locs: Vec<Locator> = self.codec.read(&mut *reader)?;
            locs
        } else {
            vec![]
        };

        Ok(Hello {
            version,
            zid,
            whatami,
            locators,
        })
    }
}
