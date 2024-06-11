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
use core::convert::TryFrom;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::ZenohIdInner;

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Length};

impl LCodec<&ZenohIdInner> for Zenoh080 {
    fn w_len(self, x: &ZenohIdInner) -> usize {
        x.size()
    }
}

impl<W> WCodec<&ZenohIdInner, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohIdInner) -> Self::Output {
        self.write(&mut *writer, &x.to_le_bytes()[..x.size()])
    }
}

impl<R> RCodec<ZenohIdInner, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohIdInner, Self::Error> {
        let size: usize = self.read(&mut *reader)?;
        if size > ZenohIdInner::MAX_SIZE {
            return Err(DidntRead);
        }
        let mut id = [0; ZenohIdInner::MAX_SIZE];
        reader.read_exact(&mut id[..size])?;
        ZenohIdInner::try_from(&id[..size]).map_err(|_| DidntRead)
    }
}

impl<W> WCodec<&ZenohIdInner, &mut W> for Zenoh080Length
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohIdInner) -> Self::Output {
        if self.length > ZenohIdInner::MAX_SIZE {
            return Err(DidntWrite);
        }
        writer.write_exact(&x.to_le_bytes()[..x.size()])
    }
}

impl<R> RCodec<ZenohIdInner, &mut R> for Zenoh080Length
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohIdInner, Self::Error> {
        if self.length > ZenohIdInner::MAX_SIZE {
            return Err(DidntRead);
        }
        let mut id = [0; ZenohIdInner::MAX_SIZE];
        reader.read_exact(&mut id[..self.length])?;
        ZenohIdInner::try_from(&id[..self.length]).map_err(|_| DidntRead)
    }
}
