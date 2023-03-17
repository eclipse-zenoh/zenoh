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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Bounded};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    SplitBuffer, ZBuf,
};
#[cfg(feature = "shared-memory")]
use {
    crate::Zenoh080Condition, core::any::TypeId, zenoh_buffers::ZSlice,
    zenoh_shm::SharedMemoryBufInfoSerialized,
};

// ZBuf bounded
macro_rules! zbuf_impl {
    ($bound:ty) => {
        impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
                self.write(&mut *writer, x.len())?;
                for s in x.zslices() {
                    writer.write_zslice(s)?;
                }
                Ok(())
            }
        }

        impl<R> RCodec<ZBuf, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
                let len: usize = self.read(&mut *reader)?;
                let mut zbuf = ZBuf::empty();
                reader.read_zslices(len, |s| zbuf.push_zslice(s))?;
                Ok(zbuf)
            }
        }
    };
}

zbuf_impl!(u8);
zbuf_impl!(u16);
zbuf_impl!(u32);
zbuf_impl!(u64);
zbuf_impl!(usize);

// ZBuf flat
impl<W> WCodec<&ZBuf, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.write(&mut *writer, x)
    }
}

impl<R> RCodec<ZBuf, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.read(&mut *reader)
    }
}

// ZBuf sliced
#[cfg(feature = "shared-memory")]
struct Zenoh080Sliced {
    codec: Zenoh080,
}

#[cfg(feature = "shared-memory")]
impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Sliced
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        self.codec.write(&mut *writer, x.zslices().count())?;

        for zs in x.zslices() {
            if zs.buf.as_any().type_id() == TypeId::of::<SharedMemoryBufInfoSerialized>() {
                self.codec
                    .write(&mut *writer, super::zslice::kind::SHM_INFO)?;
            } else {
                self.codec.write(&mut *writer, super::zslice::kind::RAW)?;
            }

            self.codec.write(&mut *writer, zs)?;
        }

        Ok(())
    }
}

#[cfg(feature = "shared-memory")]
impl<R> RCodec<ZBuf, &mut R> for Zenoh080Sliced
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let num: usize = self.codec.read(&mut *reader)?;
        let mut zbuf = ZBuf::empty();
        for _ in 0..num {
            let kind: u8 = self.codec.read(&mut *reader)?;
            match kind {
                super::zslice::kind::RAW => {
                    let len: usize = self.codec.read(&mut *reader)?;
                    reader.read_zslices(len, |s| zbuf.push_zslice(s))?;
                }
                super::zslice::kind::SHM_INFO => {
                    let bytes: Vec<u8> = self.codec.read(&mut *reader)?;
                    let shm_info: SharedMemoryBufInfoSerialized = bytes.into();
                    let zslice: ZSlice = shm_info.into();
                    zbuf.push_zslice(zslice);
                }
                _ => return Err(DidntRead),
            }
        }
        Ok(zbuf)
    }
}

#[cfg(feature = "shared-memory")]
impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Condition
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        let is_sliced = self.condition;

        if is_sliced {
            let codec = Zenoh080Sliced {
                codec: Zenoh080::new(),
            };
            codec.write(&mut *writer, x)
        } else {
            self.codec.write(&mut *writer, x)
        }
    }
}

#[cfg(feature = "shared-memory")]
impl<R> RCodec<ZBuf, &mut R> for Zenoh080Condition
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let is_sliced = self.condition;

        if is_sliced {
            let codec = Zenoh080Sliced {
                codec: Zenoh080::new(),
            };
            codec.read(&mut *reader)
        } else {
            self.codec.read(&mut *reader)
        }
    }
}
