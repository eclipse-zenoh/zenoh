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
mod hello;
mod scout;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    scouting::{id, ScoutingBody, ScoutingMessage},
};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};

impl<W> WCodec<&ScoutingMessage, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ScoutingMessage) -> Self::Output {
        let ScoutingMessage { body, .. } = x;

        match body {
            ScoutingBody::Scout(s) => self.write(&mut *writer, s),
            ScoutingBody::Hello(h) => self.write(&mut *writer, h),
        }
    }
}

impl<R> RCodec<ScoutingMessage, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ScoutingMessage, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        let body = match imsg::mid(codec.header) {
            id::SCOUT => ScoutingBody::Scout(codec.read(&mut *reader)?),
            id::HELLO => ScoutingBody::Hello(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };
        Ok(body.into())
    }
}
