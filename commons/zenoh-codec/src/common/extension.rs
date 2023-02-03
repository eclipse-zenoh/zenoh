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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf, ZSlice,
};
use zenoh_protocol::{
    common::{
        iext, imsg::has_flag, ZExtUnit, ZExtUnknown, ZExtZBuf, ZExtZInt, ZExtZSlice, ZExtensionBody,
    },
    core::ZInt,
};

impl<const ID: u8, W> WCodec<(&ZExtUnit<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtUnit<{ ID }>, bool)) -> Self::Output {
        let (_x, more) = x;
        let mut header: u8 = ID | iext::ENC_UNIT;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtUnit<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtUnit<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtUnit<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, _reader: &mut R) -> Result<(ZExtUnit<{ ID }>, bool), Self::Error> {
        if (self.header & iext::ID_MASK != ID) || (self.header & iext::ENC_MASK != iext::ENC_UNIT) {
            return Err(DidntRead);
        }

        Ok((ZExtUnit::new(), has_flag(self.header, iext::FLAG_Z)))
    }
}

impl<const ID: u8, W> WCodec<(&ZExtZInt<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZInt<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let mut header: u8 = ID | iext::ENC_ZINT;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        self.write(&mut *writer, x.value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtZInt<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZInt<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtZInt<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZInt<{ ID }>, bool), Self::Error> {
        if (self.header & iext::ID_MASK != ID) || (self.header & iext::ENC_MASK != iext::ENC_ZINT) {
            return Err(DidntRead);
        }

        let value: ZInt = self.codec.read(&mut *reader)?;

        Ok((ZExtZInt::new(value), has_flag(self.header, iext::FLAG_Z)))
    }
}

impl<const ID: u8, W> WCodec<(&ZExtZSlice<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZSlice<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let mut header: u8 = ID | iext::ENC_ZINT;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        self.write(&mut *writer, &x.value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtZSlice<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZSlice<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtZSlice<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZSlice<{ ID }>, bool), Self::Error> {
        if (self.header & iext::ID_MASK != ID) || (self.header & iext::ENC_MASK != iext::ENC_ZINT) {
            return Err(DidntRead);
        }

        let value: ZSlice = self.codec.read(&mut *reader)?;

        Ok((ZExtZSlice::new(value), has_flag(self.header, iext::FLAG_Z)))
    }
}

impl<const ID: u8, W> WCodec<(&ZExtZBuf<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZBuf<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let mut header: u8 = ID | iext::ENC_ZINT;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        self.write(&mut *writer, &x.value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtZBuf<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZBuf<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtZBuf<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZBuf<{ ID }>, bool), Self::Error> {
        if (self.header & iext::ID_MASK != ID) || (self.header & iext::ENC_MASK != iext::ENC_ZINT) {
            return Err(DidntRead);
        }

        let value: ZBuf = self.codec.read(&mut *reader)?;

        Ok((ZExtZBuf::new(value), has_flag(self.header, iext::FLAG_Z)))
    }
}

impl<W> WCodec<(&ZExtUnknown, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtUnknown, bool)) -> Self::Output {
        let (x, more) = x;
        let mut header: u8 = x.id;
        if more {
            header |= iext::FLAG_Z;
        }
        match &x.body {
            ZExtensionBody::Unit => {
                header |= iext::ENC_UNIT;
                self.write(&mut *writer, header)?
            }
            ZExtensionBody::ZInt(zint) => {
                header |= iext::ENC_ZINT;
                self.write(&mut *writer, header)?;
                self.write(&mut *writer, zint)?
            }
            ZExtensionBody::ZBuf(zbuf) => {
                header |= iext::ENC_ZBUF;
                self.write(&mut *writer, header)?;
                self.write(&mut *writer, zbuf)?
            }
        }
        Ok(())
    }
}

impl<R> RCodec<(ZExtUnknown, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtUnknown, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<R> RCodec<(ZExtUnknown, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtUnknown, bool), Self::Error> {
        let body = match self.header & iext::ENC_MASK {
            iext::ENC_UNIT => ZExtensionBody::Unit,
            iext::ENC_ZINT => {
                let zint: ZInt = self.codec.read(&mut *reader)?;
                ZExtensionBody::ZInt(zint)
            }
            iext::ENC_ZBUF => {
                let zbuf: ZBuf = self.codec.read(&mut *reader)?;
                ZExtensionBody::ZBuf(zbuf)
            }
            _ => return Err(DidntRead),
        };

        Ok((
            ZExtUnknown {
                id: self.header & iext::ID_MASK,
                body,
            },
            has_flag(self.header, iext::FLAG_Z),
        ))
    }
}
