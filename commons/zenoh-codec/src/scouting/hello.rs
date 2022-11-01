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
use crate::*;
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

impl<W> WCodec<&mut W, &Hello> for Zenoh060
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
        if let Some(wai) = x.whatami {
            if wai != WhatAmI::Router {
                header |= tmsg::flag::W;
            }
        }
        if x.locators.is_some() {
            header |= tmsg::flag::L;
        }
        zcwrite!(self, writer, header)?;

        // Body
        if let Some(zid) = x.zid.as_ref() {
            zcwrite!(self, writer, zid)?;
        }
        if let Some(wai) = x.whatami {
            if wai != WhatAmI::Router {
                let wai: ZInt = wai.into();
                zcwrite!(self, writer, wai)?;
            }
        }
        if let Some(locs) = x.locators.as_ref() {
            zcwrite!(self, writer, locs.as_slice())?;
        }
        Ok(())
    }
}

impl<R> RCodec<&mut R, Hello> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        let codec = Zenoh060RCodec {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, Hello> for Zenoh060RCodec
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Hello, Self::Error> {
        if imsg::mid(self.header) != imsg::id::HELLO {
            return Err(DidntRead);
        }

        let zid = if imsg::has_flag(self.header, tmsg::flag::I) {
            let zid: ZenohId = zcread!(self.codec, reader)?;
            Some(zid)
        } else {
            None
        };
        let whatami = if imsg::has_flag(self.header, tmsg::flag::W) {
            let wai: ZInt = zcread!(self.codec, reader)?;
            WhatAmI::try_from(wai)
        } else {
            None
        };
        let locators = if imsg::has_flag(self.header, tmsg::flag::L) {
            let locs: Vec<Locator> = zcread!(self.codec, reader)?;
            Some(locs)
        } else {
            None
        };

        Ok(Hello {
            zid,
            whatami,
            locators,
        })
    }
}
