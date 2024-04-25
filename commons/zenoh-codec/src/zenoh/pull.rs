// Copyright (c) 2024 ZettaScale Technology
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

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

use zenoh_protocol::{
    common::imsg,
    zenoh::{
        id,
        pull::{flag, Pull},
    },
};

impl<W> WCodec<&Pull, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Pull) -> Self::Output {
        let Pull { ext_unknown } = x;

        // Header
        let mut header = id::PULL;
        let mut n_exts = ext_unknown.len() as u8;
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Extensions
        for u in ext_unknown.iter() {
            n_exts -= 1;
            self.write(&mut *writer, (u, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        if imsg::mid(self.header) != id::PULL {
            return Err(DidntRead);
        }

        // Extensions
        let mut ext_unknown = Vec::new();

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let (u, ext) = extension::read(reader, "Pull", ext)?;
            ext_unknown.push(u);
            has_ext = ext;
        }

        Ok(Pull { ext_unknown })
    }
}
