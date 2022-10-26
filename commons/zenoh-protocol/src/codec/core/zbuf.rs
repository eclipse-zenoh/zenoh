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
use crate::codec::*;
use zenoh_buffers::{
    traits::{
        reader::{DidntRead, Reader},
        writer::{DidntWrite, Writer},
    },
    SplitBuffer, ZBuf,
};

impl<W> WCodec<&mut W, &ZBuf> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        for s in x.zslices() {
            writer.write_zslice(s)?;
        }
        Ok(())
    }
}

impl<R> RCodec<&mut R, ZBuf> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let len: usize = self.read(&mut *reader)?;
        let mut zbuf = ZBuf::default();
        for s in reader.read_zslices(len)? {
            zbuf.push_zslice(s);
        }
        Ok(zbuf)
    }
}
