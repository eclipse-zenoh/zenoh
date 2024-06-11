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
use zenoh_protocol::core::ZenohIdProto;

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Length};

impl LCodec<&ZenohIdProto> for Zenoh080 {
    fn w_len(self, x: &ZenohIdProto) -> usize {
        x.size()
    }
}

impl<W> WCodec<&ZenohIdProto, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohIdProto) -> Self::Output {
        self.write(&mut *writer, &x.to_le_bytes()[..x.size()])
    }
}

impl<R> RCodec<ZenohIdProto, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohIdProto, Self::Error> {
        let size: usize = self.read(&mut *reader)?;
        if size > ZenohIdProto::MAX_SIZE {
            return Err(DidntRead);
        }
        let mut id = [0; ZenohIdProto::MAX_SIZE];
        reader.read_exact(&mut id[..size])?;
        ZenohIdProto::try_from(&id[..size]).map_err(|_| DidntRead)
    }
}

impl<W> WCodec<&ZenohIdProto, &mut W> for Zenoh080Length
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohIdProto) -> Self::Output {
        if self.length > ZenohIdProto::MAX_SIZE {
            return Err(DidntWrite);
        }
        writer.write_exact(&x.to_le_bytes()[..x.size()])
    }
}

impl<R> RCodec<ZenohIdProto, &mut R> for Zenoh080Length
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohIdProto, Self::Error> {
        if self.length > ZenohIdProto::MAX_SIZE {
            return Err(DidntRead);
        }
        let mut id = [0; ZenohIdProto::MAX_SIZE];
        reader.read_exact(&mut id[..self.length])?;
        ZenohIdProto::try_from(&id[..self.length]).map_err(|_| DidntRead)
    }
}
