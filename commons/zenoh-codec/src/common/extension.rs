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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Condition};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, ZExtUnit, ZExtUnknown, ZExtZBuf, ZExtZInt, ZExtensionBody},
    core::ZInt,
};

impl<const ID: u8, W> WCodec<&ZExtUnit<{ ID }>, &mut W> for Zenoh060Condition
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, _x: &ZExtUnit<{ ID }>) -> Self::Output {
        let mut header: u8 = ID | iext::ENC_UNIT;
        if self.condition {
            header |= iext::FLAG_Z;
        }
        self.codec.write(&mut *writer, header)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<ZExtUnit<{ ID }>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZExtUnit<{ ID }>, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        if (header & iext::ID_MASK != ID) || (header & iext::ENC_MASK != iext::ENC_UNIT) {
            return Err(DidntRead);
        }

        Ok(ZExtUnit::new())
    }
}

impl<const ID: u8, W> WCodec<&ZExtZInt<{ ID }>, &mut W> for Zenoh060Condition
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZExtZInt<{ ID }>) -> Self::Output {
        let mut header: u8 = ID | iext::ENC_ZINT;
        if self.condition {
            header |= iext::FLAG_Z;
        }
        self.codec.write(&mut *writer, header)?;
        self.codec.write(&mut *writer, x.value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<ZExtZInt<{ ID }>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZExtZInt<{ ID }>, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        if (header & iext::ID_MASK != ID) || (header & iext::ENC_MASK != iext::ENC_ZINT) {
            return Err(DidntRead);
        }

        let value: ZInt = self.read(&mut *reader)?;

        Ok(ZExtZInt::new(value))
    }
}

impl<const ID: u8, W> WCodec<&ZExtZBuf<{ ID }>, &mut W> for Zenoh060Condition
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZExtZBuf<{ ID }>) -> Self::Output {
        let mut header: u8 = ID | iext::ENC_ZINT;
        if self.condition {
            header |= iext::FLAG_Z;
        }
        self.codec.write(&mut *writer, header)?;
        self.codec.write(&mut *writer, &x.value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<ZExtZBuf<{ ID }>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZExtZBuf<{ ID }>, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        if (header & iext::ID_MASK != ID) || (header & iext::ENC_MASK != iext::ENC_ZINT) {
            return Err(DidntRead);
        }

        let value: ZBuf = self.read(&mut *reader)?;

        Ok(ZExtZBuf::new(value))
    }
}

impl<W> WCodec<&ZExtUnknown, &mut W> for Zenoh060Condition
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZExtUnknown) -> Self::Output {
        let mut header: u8 = x.id;
        if self.condition {
            header |= iext::FLAG_Z;
        }
        match &x.body {
            ZExtensionBody::Unit => {
                header |= iext::ENC_UNIT;
                self.codec.write(&mut *writer, header)?
            }
            ZExtensionBody::ZInt(zint) => {
                header |= iext::ENC_ZINT;
                self.codec.write(&mut *writer, header)?;
                self.codec.write(&mut *writer, zint)?
            }
            ZExtensionBody::ZBuf(zbuf) => {
                header |= iext::ENC_ZBUF;
                self.codec.write(&mut *writer, header)?;
                self.codec.write(&mut *writer, zbuf)?
            }
        }
        Ok(())
    }
}

impl<R> RCodec<ZExtUnknown, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZExtUnknown, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let body = match header & iext::ENC_MASK {
            iext::ENC_UNIT => ZExtensionBody::Unit,
            iext::ENC_ZINT => {
                let zint: ZInt = self.read(&mut *reader)?;
                ZExtensionBody::ZInt(zint)
            }
            iext::ENC_ZBUF => {
                let zbuf: ZBuf = self.read(&mut *reader)?;
                ZExtensionBody::ZBuf(zbuf)
            }
            _ => return Err(DidntRead),
        };

        Ok(ZExtUnknown {
            id: header & iext::ID_MASK,
            body,
        })
    }
}
