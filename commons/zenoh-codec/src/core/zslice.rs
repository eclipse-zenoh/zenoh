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
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Bounded};

// ZSlice - Bounded
macro_rules! zslice_impl {
    ($bound:ty) => {
        impl<W> WCodec<&ZSlice, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &ZSlice) -> Self::Output {
                self.write(&mut *writer, x.len())?;
                writer.write_zslice(x)?;
                Ok(())
            }
        }

        impl<R> RCodec<ZSlice, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            #[allow(clippy::uninit_vec)]
            fn read(self, reader: &mut R) -> Result<ZSlice, Self::Error> {
                let len: usize = self.read(&mut *reader)?;
                let zslice = reader.read_zslice(len)?;
                Ok(zslice)
            }
        }
    };
}

zslice_impl!(u8);
zslice_impl!(u16);
zslice_impl!(u32);
zslice_impl!(u64);
zslice_impl!(usize);

// ZSlice
impl<W> WCodec<&ZSlice, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZSlice) -> Self::Output {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.write(&mut *writer, x)
    }
}

impl<R> RCodec<ZSlice, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZSlice, Self::Error> {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.read(&mut *reader)
    }
}
