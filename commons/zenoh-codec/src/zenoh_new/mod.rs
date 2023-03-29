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
pub mod del;
pub mod put;
pub mod query;

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    zenoh_new::{id, PushBody, RequestBody},
};

// Push
impl<W> WCodec<&PushBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PushBody) -> Self::Output {
        match x {
            PushBody::Put(b) => self.write(&mut *writer, b),
            PushBody::Del(b) => self.write(&mut *writer, b),
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
            id::DEL => PushBody::Del(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}

// Request
impl<W> WCodec<&RequestBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &RequestBody) -> Self::Output {
        match x {
            RequestBody::Query(b) => self.write(&mut *writer, b),
            RequestBody::Put(b) => self.write(&mut *writer, b),
            RequestBody::Del(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<RequestBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<RequestBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::QUERY => RequestBody::Query(codec.read(&mut *reader)?),
            id::PUT => RequestBody::Put(codec.read(&mut *reader)?),
            id::DEL => RequestBody::Del(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}
