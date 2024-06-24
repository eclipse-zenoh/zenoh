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
mod encoding;
mod locator;
#[cfg(feature = "shared-memory")]
mod shm;
mod timestamp;
mod wire_expr;
mod zbuf;
mod zenohid;
mod zint;
mod zslice;

use alloc::{string::String, vec::Vec};

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};

// [u8; N]
macro_rules! array_impl {
    ($n:expr) => {
        impl<W> WCodec<[u8; $n], &mut W> for Zenoh080
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: [u8; $n]) -> Self::Output {
                writer.write_exact(x.as_slice())
            }
        }

        impl<W> WCodec<&[u8; $n], &mut W> for Zenoh080
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &[u8; $n]) -> Self::Output {
                self.write(writer, *x)
            }
        }

        impl<R> RCodec<[u8; $n], &mut R> for Zenoh080
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<[u8; $n], Self::Error> {
                let mut x = [0u8; $n];
                reader.read_exact(&mut x)?;
                Ok(x)
            }
        }

        impl LCodec<[u8; $n]> for Zenoh080 {
            fn w_len(self, _: [u8; $n]) -> usize {
                $n
            }
        }
    };
}

array_impl!(1);
array_impl!(2);
array_impl!(3);
array_impl!(4);
array_impl!(5);
array_impl!(6);
array_impl!(7);
array_impl!(8);
array_impl!(9);
array_impl!(10);
array_impl!(11);
array_impl!(12);
array_impl!(13);
array_impl!(14);
array_impl!(15);
array_impl!(16);

// &[u8] / Vec<u8> - Bounded
macro_rules! vec_impl {
    ($bound:ty) => {
        impl<W> WCodec<&[u8], &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &[u8]) -> Self::Output {
                self.write(&mut *writer, x.len())?;
                if x.is_empty() {
                    Ok(())
                } else {
                    writer.write_exact(x)
                }
            }
        }

        impl<R> RCodec<Vec<u8>, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            #[allow(clippy::uninit_vec)]
            fn read(self, reader: &mut R) -> Result<Vec<u8>, Self::Error> {
                let len: usize = self.read(&mut *reader)?;
                let mut buff = zenoh_buffers::vec::uninit(len);
                if len != 0 {
                    reader.read_exact(&mut buff[..])?;
                }
                Ok(buff)
            }
        }
    };
}

vec_impl!(u8);
vec_impl!(u16);
vec_impl!(u32);
vec_impl!(u64);
vec_impl!(usize);

// &[u8] / Vec<u8>
impl LCodec<&[u8]> for Zenoh080 {
    fn w_len(self, x: &[u8]) -> usize {
        self.w_len(x.len()) + x.len()
    }
}

impl<W> WCodec<&[u8], &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[u8]) -> Self::Output {
        let zcodec = Zenoh080Bounded::<usize>::new();
        zcodec.write(&mut *writer, x)
    }
}

impl<R> RCodec<Vec<u8>, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Vec<u8>, Self::Error> {
        let zcodec = Zenoh080Bounded::<usize>::new();
        zcodec.read(&mut *reader)
    }
}

// &str / String - Bounded
macro_rules! str_impl {
    ($bound:ty) => {
        impl<W> WCodec<&str, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &str) -> Self::Output {
                self.write(&mut *writer, x.as_bytes())
            }
        }

        impl<W> WCodec<&String, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &String) -> Self::Output {
                self.write(&mut *writer, x.as_str())
            }
        }

        impl<R> RCodec<String, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            #[allow(clippy::uninit_vec)]
            fn read(self, reader: &mut R) -> Result<String, Self::Error> {
                let vec: Vec<u8> = self.read(&mut *reader)?;
                String::from_utf8(vec).map_err(|_| DidntRead)
            }
        }
    };
}

str_impl!(u8);
str_impl!(u16);
str_impl!(u32);
str_impl!(u64);
str_impl!(usize);

// &str / String
impl LCodec<&str> for Zenoh080 {
    fn w_len(self, x: &str) -> usize {
        self.w_len(x.as_bytes())
    }
}

impl LCodec<&String> for Zenoh080 {
    fn w_len(self, x: &String) -> usize {
        self.w_len(x.as_bytes())
    }
}

impl<W> WCodec<&str, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &str) -> Self::Output {
        let zcodec = Zenoh080Bounded::<usize>::new();
        zcodec.write(&mut *writer, x)
    }
}

impl<W> WCodec<&String, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &String) -> Self::Output {
        let zcodec = Zenoh080Bounded::<usize>::new();
        zcodec.write(&mut *writer, x)
    }
}

impl<R> RCodec<String, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<String, Self::Error> {
        let zcodec = Zenoh080Bounded::<usize>::new();
        zcodec.read(&mut *reader)
    }
}
