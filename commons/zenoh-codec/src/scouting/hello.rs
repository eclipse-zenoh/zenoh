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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header};
use alloc::{vec, vec::Vec};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{Locator, WhatAmI, ZInt, ZenohId},
    scouting::Hello,
    transport::tmsg,
};

impl<W> WCodec<&Hello, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Hello) -> Self::Output {
        // Header
        let mut header = tmsg::id::HELLO;
        if x.zid.is_some() {
            header |= tmsg::flag::I
        }
        if x.whatami != WhatAmI::Router {
            header |= tmsg::flag::W;
        }
        if !x.locators.is_empty() {
            header |= tmsg::flag::L;
        }
        self.write(&mut *writer, header)?;

        // Body
        if let Some(zid) = x.zid.as_ref() {
            self.write(&mut *writer, zid)?;
        }
        if x.whatami != WhatAmI::Router {
            let wai: ZInt = x.whatami.into();
            self.write(&mut *writer, wai)?;
        }
        if !x.locators.is_empty() {
            self.write(&mut *writer, x.locators.as_slice())?;
        }
        Ok(())
    }
}

impl<R> RCodec<Hello, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Hello, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        if imsg::mid(self.header) != imsg::id::HELLO {
            return Err(DidntRead);
        }

        let zid = if imsg::has_flag(self.header, tmsg::flag::I) {
            let zid: ZenohId = self.codec.read(&mut *reader)?;
            Some(zid)
        } else {
            None
        };
        let whatami = if imsg::has_flag(self.header, tmsg::flag::W) {
            let wai: ZInt = self.codec.read(&mut *reader)?;
            WhatAmI::try_from(wai).ok_or(DidntRead)?
        } else {
            WhatAmI::Router
        };
        let locators = if imsg::has_flag(self.header, tmsg::flag::L) {
            let locs: Vec<Locator> = self.codec.read(&mut *reader)?;
            locs
        } else {
            vec![]
        };

        Ok(Hello {
            zid,
            whatami,
            locators,
        })
    }
}
