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
    buffer::Buffer,
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};

// ZBuf bounded
macro_rules! zbuf_impl {
    ($bound:ty) => {
        impl LCodec<&ZBuf> for Zenoh080Bounded<$bound> {
            fn w_len(self, message: &ZBuf) -> usize {
                message.len()
            }
        }

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

impl LCodec<&ZBuf> for Zenoh080 {
    fn w_len(self, message: &ZBuf) -> usize {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.w_len(message)
    }
}

// ZBuf sliced
#[cfg(feature = "shared-memory")]
mod shm {
    use zenoh_buffers::{ZSlice, ZSliceKind};
    use zenoh_shm::ShmBufInner;

    use super::*;
    use crate::Zenoh080Sliced;

    const RAW: u8 = 0;
    const SHM_PTR: u8 = 1;

    macro_rules! zbuf_sliced_impl {
        ($bound:ty) => {
            impl LCodec<&ZBuf> for Zenoh080Sliced<$bound> {
                fn w_len(self, message: &ZBuf) -> usize {
                    if self.is_sliced {
                        message.zslices().fold(0, |acc, x| acc + 1 + x.len())
                    } else {
                        self.codec.w_len(message)
                    }
                }
            }

            impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Sliced<$bound>
            where
                W: Writer,
            {
                type Output = Result<(), DidntWrite>;

                fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
                    if self.is_sliced {
                        self.codec.write(&mut *writer, x.zslices().count())?;

                        for zs in x.zslices() {
                            match zs.kind {
                                ZSliceKind::Raw => {
                                    self.codec.write(&mut *writer, RAW)?;
                                    self.codec.write(&mut *writer, zs)?;
                                }
                                ZSliceKind::ShmPtr => {
                                    self.codec.write(&mut *writer, SHM_PTR)?;
                                    let shmb = zs.downcast_ref::<ShmBufInner>().unwrap();
                                    let mut info = vec![];
                                    Zenoh080::new().write(&mut &mut info, &shmb.info)?;
                                    self.codec.write(&mut *writer, &*info)?;
                                    // Increase the reference count so to keep the ShmBufInner
                                    // valid until it is received.
                                    unsafe { shmb.inc_ref_count() };
                                }
                            }
                        }
                    } else {
                        self.codec.write(&mut *writer, x)?;
                    }

                    Ok(())
                }
            }

            impl<R> RCodec<ZBuf, &mut R> for Zenoh080Sliced<$bound>
            where
                R: Reader,
            {
                type Error = DidntRead;

                fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
                    if self.is_sliced {
                        let num: usize = self.codec.read(&mut *reader)?;
                        let mut zbuf = ZBuf::empty();
                        for _ in 0..num {
                            let kind: u8 = self.codec.read(&mut *reader)?;
                            match kind {
                                RAW => {
                                    let len: usize = self.codec.read(&mut *reader)?;
                                    reader.read_zslices(len, |s| zbuf.push_zslice(s))?;
                                }
                                SHM_PTR => {
                                    let mut zslice: ZSlice = self.codec.read(&mut *reader)?;
                                    zslice.kind = ZSliceKind::ShmPtr;
                                    zbuf.push_zslice(zslice);
                                }
                                _ => return Err(DidntRead),
                            }
                        }
                        Ok(zbuf)
                    } else {
                        self.codec.read(&mut *reader)
                    }
                }
            }
        };
    }

    zbuf_sliced_impl!(u8);
    zbuf_sliced_impl!(u16);
    zbuf_sliced_impl!(u32);
    zbuf_sliced_impl!(u64);
    zbuf_sliced_impl!(usize);
}
