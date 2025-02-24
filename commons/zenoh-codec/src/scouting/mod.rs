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
    ZSlice,
};
use zenoh_protocol::{
    common::{imsg, ZExtZBufHeader},
    scouting::{ext::GroupsType, id, ScoutingBody, ScoutingMessage},
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header};

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

impl<const ID: u8, W> WCodec<(&GroupsType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&GroupsType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;

        // Compute the total extension length
        let w_len = x.iter().fold(0_usize, |len, zs| len + self.w_len(zs));
        // Write the extension header
        let header: ZExtZBufHeader<ID> = ZExtZBufHeader::new(w_len);
        self.write(&mut *writer, (&header, more))?;
        // Write the extension body
        for zs in x.iter() {
            self.write(&mut *writer, zs)?;
        }

        Ok(())
    }
}

impl<const ID: u8, R> RCodec<GroupsType<{ ID }>, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<GroupsType<{ ID }>, Self::Error> {
        let mut groups = Vec::new();

        while reader.can_read() {
            let zs: ZSlice = self.read(&mut *reader)?;
            groups.push(zs);
        }

        Ok(GroupsType::new(groups))
    }
}
