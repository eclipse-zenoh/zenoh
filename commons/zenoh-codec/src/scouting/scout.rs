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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{whatami::WhatAmIMatcher, ZInt},
    scouting::Scout,
    transport::tmsg,
};

impl<W> WCodec<&Scout, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Scout) -> Self::Output {
        // Header
        let mut header = tmsg::id::SCOUT;
        if x.zid_request {
            header |= tmsg::flag::I;
        }
        if x.what.is_some() {
            header |= tmsg::flag::W;
        }
        self.write(&mut *writer, header)?;

        // Body
        match x.what {
            Some(w) => {
                let w: ZInt = w.into();
                self.write(&mut *writer, w)
            }
            None => Ok(()),
        }
    }
}

impl<R> RCodec<Scout, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Scout, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Scout, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Scout, Self::Error> {
        if imsg::mid(self.header) != imsg::id::SCOUT {
            return Err(DidntRead);
        }

        let zid_request = imsg::has_flag(self.header, tmsg::flag::I);
        let what = if imsg::has_flag(self.header, tmsg::flag::W) {
            let wai: ZInt = self.codec.read(reader)?;
            let wai: ZInt = wai | 0x80; // FIXME: This fixes a misalignment with Zenoh-Pico, but it is implemented as a workaround because:
                                        //          1) the new protocol will fix it as intended
                                        //          2) we want to avoid breaking older builds, while fixing the misalignment with Zenoh-Pico
            WhatAmIMatcher::try_from(wai)
        } else {
            None
        };
        Ok(Scout { what, zid_request })
    }
}
