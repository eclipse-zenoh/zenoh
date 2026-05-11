//
// Copyright (c) 2026 ZettaScale Technology
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
use alloc::vec::Vec;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::ZExtZBufHeader,
    network::timestamp_stack::{Interception, TimestampStack, TsStackType},
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header};

// Interception
impl LCodec<&Interception> for Zenoh080 {
    fn w_len(self, x: &Interception) -> usize {
        1 + self.w_len(&x.timestamp[..])
    }
}

impl<W> WCodec<&Interception, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Interception) -> Self::Output {
        self.write(&mut *writer, x.flags)?;
        self.write(&mut *writer, &x.timestamp[..])
    }
}

impl<R> RCodec<Interception, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Interception, Self::Error> {
        let flags: u8 = self.read(&mut *reader)?;
        let timestamp: Vec<u8> = self.read(&mut *reader)?;
        Ok(Interception { flags, timestamp })
    }
}

// TimestampStack
impl LCodec<&TimestampStack> for Zenoh080 {
    fn w_len(self, x: &TimestampStack) -> usize {
        let mut len = 1; // conf_flags
        len += self.w_len(x.stack.len());
        for i in &x.stack {
            len += self.w_len(i);
        }
        len
    }
}

impl<W> WCodec<&TimestampStack, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TimestampStack) -> Self::Output {
        self.write(&mut *writer, x.conf_flags)?;
        self.write(&mut *writer, x.stack.len())?;
        for i in &x.stack {
            self.write(&mut *writer, i)?;
        }
        Ok(())
    }
}

impl<R> RCodec<TimestampStack, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TimestampStack, Self::Error> {
        let conf_flags: u8 = self.read(&mut *reader)?;
        let count: usize = self.read(&mut *reader)?;
        //FIXME: this can panic due to call with unsanitized count input
        let mut stack: Vec<Interception> = Vec::with_capacity(count);
        for _ in 0..count {
            stack.push(self.read(&mut *reader)?);
        }
        Ok(TimestampStack { conf_flags, stack })
    }
}

// TsStackType extension wrapper
impl<const ID: u8> LCodec<&TsStackType<{ ID }>> for Zenoh080 {
    fn w_len(self, x: &TsStackType<{ ID }>) -> usize {
        self.w_len(&x.ts_stack)
    }
}

impl<W, const ID: u8> WCodec<(&TsStackType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&TsStackType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;
        self.write(&mut *writer, &x.ts_stack)
    }
}

impl<R, const ID: u8> RCodec<(TsStackType<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(TsStackType<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R, const ID: u8> RCodec<(TsStackType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(TsStackType<{ ID }>, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ID }>, bool) = self.read(&mut *reader)?;
        let ts_stack: TimestampStack = self.codec.read(&mut *reader)?;
        Ok((TsStackType { ts_stack }, more))
    }
}
