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
pub mod put;
use zenoh_buffers::reader::{DidntRead, Reader};
use zenoh_buffers::writer::{DidntWrite, Writer};
use zenoh_protocol::common::imsg;
use zenoh_protocol::zenoh_new::{id, PushBody};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};

// PushBody
impl<W> WCodec<&PushBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PushBody) -> Self::Output {
        match x {
            PushBody::Put(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<PushBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<PushBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::PUT => PushBody::Put(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}
