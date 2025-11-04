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
use alloc::vec::Vec;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::common::{
    iext, imsg::has_flag, ZExtBody, ZExtUnit, ZExtUnknown, ZExtZ64, ZExtZBuf, ZExtZBufHeader,
};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header};

fn read_inner<R>(reader: &mut R, _s: &str, header: u8) -> Result<(ZExtUnknown, bool), DidntRead>
where
    R: Reader,
{
    let codec = Zenoh080Header::new(header);
    let (u, has_ext): (ZExtUnknown, bool) = codec.read(&mut *reader)?;
    if u.is_mandatory() {
        #[cfg(feature = "std")]
        tracing::error!("Unknown {_s} ext: {u:?}");
        return Err(DidntRead);
    } else {
        #[cfg(feature = "std")]
        tracing::debug!("Unknown {_s} ext: {u:?}");
    }
    Ok((u, has_ext))
}

#[cold]
#[inline(never)]
pub fn read<R>(reader: &mut R, s: &str, header: u8) -> Result<(ZExtUnknown, bool), DidntRead>
where
    R: Reader,
{
    read_inner(&mut *reader, s, header)
}

fn skip_inner<R>(reader: &mut R, s: &str, header: u8) -> Result<bool, DidntRead>
where
    R: Reader,
{
    let (_, has_ext): (ZExtUnknown, bool) = read_inner(&mut *reader, s, header)?;
    Ok(has_ext)
}

#[cold]
#[inline(never)]
pub fn skip<R>(reader: &mut R, s: &str, header: u8) -> Result<bool, DidntRead>
where
    R: Reader,
{
    skip_inner(reader, s, header)
}

#[cold]
#[inline(never)]
pub fn skip_all<R>(reader: &mut R, s: &str) -> Result<(), DidntRead>
where
    R: Reader,
{
    let codec = Zenoh080::new();
    let mut has_ext = true;
    while has_ext {
        let header: u8 = codec.read(&mut *reader)?;
        has_ext = skip_inner(reader, s, header)?;
    }
    Ok(())
}

// ZExtUnit
impl<const ID: u8, W> WCodec<(&ZExtUnit<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtUnit<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ZExtUnit = x;

        let mut header: u8 = ID;
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
        if iext::eid(self.header) != ID {
            return Err(DidntRead);
        }
        Ok((ZExtUnit::new(), has_flag(self.header, iext::FLAG_Z)))
    }
}

// ZExtZ64
impl<const ID: u8, W> WCodec<(&ZExtZ64<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZ64<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ZExtZ64 { value } = x;

        let mut header: u8 = ID;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        self.write(&mut *writer, value)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtZ64<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZ64<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtZ64<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZ64<{ ID }>, bool), Self::Error> {
        if iext::eid(self.header) != ID {
            return Err(DidntRead);
        }

        let value: u64 = self.codec.read(&mut *reader)?;
        Ok((ZExtZ64::new(value), has_flag(self.header, iext::FLAG_Z)))
    }
}

// ZExtZBuf
impl<const ID: u8, W> WCodec<(&ZExtZBuf<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZBuf<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ZExtZBuf { value } = x;

        let mut header: u8 = ID;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        let bodec = Zenoh080Bounded::<u32>::new();
        bodec.write(&mut *writer, value)?;
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
        if iext::eid(self.header) != ID {
            return Err(DidntRead);
        }
        let bodec = Zenoh080Bounded::<u32>::new();
        let value: ZBuf = bodec.read(&mut *reader)?;
        Ok((ZExtZBuf::new(value), has_flag(self.header, iext::FLAG_Z)))
    }
}

// ZExtZBufHeader
impl<const ID: u8, W> WCodec<(&ZExtZBufHeader<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtZBufHeader<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ZExtZBufHeader { len } = x;

        let mut header: u8 = ID;
        if more {
            header |= iext::FLAG_Z;
        }
        self.write(&mut *writer, header)?;
        let bodec = Zenoh080Bounded::<u32>::new();
        bodec.write(&mut *writer, *len)?;
        Ok(())
    }
}

impl<const ID: u8, R> RCodec<(ZExtZBufHeader<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZBufHeader<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<const ID: u8, R> RCodec<(ZExtZBufHeader<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ZExtZBufHeader<{ ID }>, bool), Self::Error> {
        if iext::eid(self.header) != ID {
            return Err(DidntRead);
        }

        let bodec = Zenoh080Bounded::<u32>::new();
        let len: usize = bodec.read(&mut *reader)?;
        Ok((
            ZExtZBufHeader::new(len),
            has_flag(self.header, iext::FLAG_Z),
        ))
    }
}

// ZExtUnknown
impl<W> WCodec<(&ZExtUnknown, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ZExtUnknown, bool)) -> Self::Output {
        let (x, more) = x;
        let ZExtUnknown { id, body } = x;

        let mut header: u8 = *id;
        if more {
            header |= iext::FLAG_Z;
        }
        match body {
            ZExtBody::Unit => self.write(&mut *writer, header)?,
            ZExtBody::Z64(u64) => {
                self.write(&mut *writer, header)?;
                self.write(&mut *writer, *u64)?
            }
            ZExtBody::ZBuf(zbuf) => {
                self.write(&mut *writer, header)?;
                let bodec = Zenoh080Bounded::<u32>::new();
                bodec.write(&mut *writer, zbuf)?
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
            iext::ENC_UNIT => ZExtBody::Unit,
            iext::ENC_Z64 => {
                let u64: u64 = self.codec.read(&mut *reader)?;
                ZExtBody::Z64(u64)
            }
            iext::ENC_ZBUF => {
                let bodec = Zenoh080Bounded::<u32>::new();
                let zbuf: ZBuf = bodec.read(&mut *reader)?;
                ZExtBody::ZBuf(zbuf)
            }
            _ => {
                return Err(DidntRead);
            }
        };

        Ok((
            ZExtUnknown {
                id: self.header & !iext::FLAG_Z,
                body,
            },
            has_flag(self.header, iext::FLAG_Z),
        ))
    }
}

impl<W> WCodec<&[ZExtUnknown], &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[ZExtUnknown]) -> Self::Output {
        let len = x.len();
        for (i, e) in x.iter().enumerate() {
            self.write(&mut *writer, (e, i < len - 1))?;
        }
        Ok(())
    }
}

impl<R> RCodec<Vec<ZExtUnknown>, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Vec<ZExtUnknown>, Self::Error> {
        let mut exts = Vec::new();
        let mut has_ext = reader.can_read();
        while has_ext {
            let (e, more): (ZExtUnknown, bool) = self.read(&mut *reader)?;
            exts.push(e);
            has_ext = more;
        }
        Ok(exts)
    }
}

// Macros
#[macro_export]
macro_rules! impl_zextz64 {
    ($ext:ty, $id:expr) => {
        impl<W> WCodec<($ext, bool), &mut W> for Zenoh080
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: ($ext, bool)) -> Self::Output {
                let (x, more) = x;
                let ext: ZExtZ64<{ $id }> = x.into();
                self.write(&mut *writer, (&ext, more))
            }
        }

        impl<R> RCodec<($ext, bool), &mut R> for Zenoh080
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<($ext, bool), Self::Error> {
                let header: u8 = self.read(&mut *reader)?;
                let codec = Zenoh080Header::new(header);
                codec.read(reader)
            }
        }

        impl<R> RCodec<($ext, bool), &mut R> for Zenoh080Header
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<($ext, bool), Self::Error> {
                let (ext, more): (ZExtZ64<{ $id }>, bool) = self.read(&mut *reader)?;
                Ok((ext.into(), more))
            }
        }
    };
}
