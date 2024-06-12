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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::encoding::{flag, Encoding, EncodingId},
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};

impl LCodec<&Encoding> for Zenoh080 {
    fn w_len(self, x: &Encoding) -> usize {
        let (id, schema) = x.id_and_schema();
        let mut len = self.w_len((id as u32) << 1);
        if let Some(schema) = schema {
            len += self.w_len(schema);
        }
        len
    }
}

impl<W> WCodec<&Encoding, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Encoding) -> Self::Output {
        let (id, schema) = x.id_and_schema();
        let mut id = (id as u32) << 1;

        if schema.is_some() {
            id |= flag::S;
        }
        let zodec = Zenoh080Bounded::<u32>::new();
        zodec.write(&mut *writer, id)?;
        if let Some(schema) = schema {
            let zodec = Zenoh080Bounded::<u8>::new();
            zodec.write(&mut *writer, schema)?;
        }
        Ok(())
    }
}

impl<R> RCodec<Encoding, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Encoding, Self::Error> {
        let zodec = Zenoh080Bounded::<u32>::new();
        let id: u32 = zodec.read(&mut *reader)?;
        let (id, has_schema) = (
            (id >> 1) as EncodingId,
            imsg::has_flag(id as u8, flag::S as u8),
        );

        let mut encoding = Encoding::new(id);
        if has_schema {
            let zodec = Zenoh080Bounded::<u8>::new();
            let schema: String = zodec.read(&mut *reader)?;
            encoding = encoding.with_schema(schema);
        }

        Ok(encoding)
    }
}
