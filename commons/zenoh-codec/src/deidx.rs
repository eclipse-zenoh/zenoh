//
// Copyright (c) 2025 ZettaScale Technology
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
use crate::{RCodec, Zenoh080};
use ::core::{fmt, ops::Deref};
use zenoh_buffers::reader::Reader;

// --
pub struct MsgIdx<T> {
    pub msg: T,
    pub idx: usize,
    pub len: usize,
}

impl<T> Deref for MsgIdx<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.msg
    }
}

impl<T> fmt::Debug for MsgIdx<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "msg: {:?}, idx: {}, len: {}",
            self.msg, self.idx, self.len
        )
    }
}

// --
trait DeIdx {
    type MessageIdx;
}

macro_rules! impl_codec_index_for_primitive_type {
    ($t:ty) => {
        impl DeIdx for $t {
            type MessageIdx = $t;
        }
    };
}

impl_codec_index_for_primitive_type!(bool);
impl_codec_index_for_primitive_type!(u8);
impl_codec_index_for_primitive_type!(u16);
impl_codec_index_for_primitive_type!(u32);
impl_codec_index_for_primitive_type!(u64);
impl_codec_index_for_primitive_type!(u128);
impl_codec_index_for_primitive_type!(usize);
impl_codec_index_for_primitive_type!(i8);
impl_codec_index_for_primitive_type!(i16);
impl_codec_index_for_primitive_type!(i32);
impl_codec_index_for_primitive_type!(i64);
impl_codec_index_for_primitive_type!(i128);
impl_codec_index_for_primitive_type!(isize);
impl_codec_index_for_primitive_type!(f32);
impl_codec_index_for_primitive_type!(f64);
impl_codec_index_for_primitive_type!(char);
impl_codec_index_for_primitive_type!(String);

impl<A, B> DeIdx for (A, B) {
    type MessageIdx = (MsgIdx<A>, MsgIdx<B>);
}

impl<A, B, C> DeIdx for (A, B, C) {
    type MessageIdx = (MsgIdx<A>, MsgIdx<B>, MsgIdx<C>);
}

impl<A, B, C, D> DeIdx for (A, B, C, D) {
    type MessageIdx = (MsgIdx<A>, MsgIdx<B>, MsgIdx<C>, MsgIdx<D>);
}

impl<T> DeIdx for Vec<T>
where
    T: DeIdx,
{
    type MessageIdx = Vec<MsgIdx<T>>;
}

impl<T, const N: usize> DeIdx for [T; N]
where
    T: DeIdx,
{
    type MessageIdx = [MsgIdx<T>; N];
}

// --
pub trait RCodecIndex<Message, Buffer> {
    type Error;
    fn read(self, buffer: Buffer) -> Result<Message, Self::Error>;
}

pub struct Zenoh080Index {
    inner: Zenoh080,
    pub idx: usize,
}

impl Zenoh080Index {
    pub fn new() -> Self {
        Self {
            inner: Zenoh080::new(),
            idx: 0,
        }
    }
}

impl Default for Zenoh080Index {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct DidnReadDeIdx {
    pub idx: usize,
    pub msg: &'static str,
}

impl<Message, R> RCodecIndex<MsgIdx<Message>, &mut R> for &mut Zenoh080Index
where
    Message: 'static,
    R: Reader,
    for<'a> Zenoh080: RCodec<Message, &'a mut R>,
{
    type Error = DidnReadDeIdx;

    fn read(self, reader: &mut R) -> Result<MsgIdx<Message>, Self::Error> {
        let start = reader.remaining();
        let msg = self.inner.read(reader).map_err(|_| DidnReadDeIdx {
            idx: self.idx,
            msg: core::any::type_name::<Message>(),
        })?;
        let end = reader.remaining();

        let len = start - end;
        let msg = MsgIdx {
            msg,
            idx: self.idx,
            len,
        };
        self.idx += len;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::{MsgIdx, RCodec, RCodecIndex, Zenoh080Index};
    use crate::{LCodec, WCodec, Zenoh080};
    use zenoh_buffers::reader::{HasReader, Reader};
    use zenoh_buffers::writer::{HasWriter, Writer};

    #[test]
    fn msgidx() {
        let v_in: Vec<usize> = vec![42, 43, 44, 200, usize::MAX, 45];

        let mut buff = vec![];
        let w_codec = Zenoh080::new();
        let mut writer = buff.writer();

        for v in v_in.iter() {
            w_codec.write(&mut writer, *v).unwrap();
        }

        let r_codec = Zenoh080::new();
        let mut reader = buff.reader();

        println!("Baseline");
        for v in v_in.iter() {
            print!("- Decoding {}\t", v);
            let v_out: usize = r_codec.read(&mut reader).unwrap();
            println!("...decoded {}", v_out);
            assert_eq!(*v, v_out);
        }

        let mut r_codec = Zenoh080Index::new();
        let mut reader = buff.reader();

        println!("Wrapper");
        let mut idx = 0;
        for v in v_in.iter() {
            print!("- Decoding {}\t", v);
            let v_out: MsgIdx<usize> = r_codec.read(&mut reader).unwrap();
            println!("...decoded {:?}", v_out);
            assert_eq!(*v, v_out.msg);
            assert_eq!(v_out.idx, idx);
            assert_eq!(v_out.len, w_codec.w_len(*v));
            idx += w_codec.w_len(*v);
        }

        assert!(!reader.can_read());
    }

    #[derive(Debug, PartialEq)]
    struct A {
        one: usize,
        b: B,
    }

    impl<W> WCodec<&A, &mut W> for Zenoh080
    where
        W: Writer,
    {
        type Output = Result<(), ()>;

        fn write(self, writer: &mut W, message: &A) -> Self::Output {
            self.write(&mut *writer, message.one).map_err(|_| ())?;
            self.write(&mut *writer, &message.b).map_err(|_| ())?;
            Ok(())
        }
    }

    impl<R> RCodec<A, &mut R> for Zenoh080
    where
        R: Reader,
    {
        type Error = ();

        fn read(self, reader: &mut R) -> Result<A, Self::Error> {
            let one: usize = self.read(&mut *reader).map_err(|_| ())?;
            let b: B = self.read(&mut *reader).map_err(|_| ())?;
            Ok(A { one, b })
        }
    }

    #[derive(Debug, PartialEq)]
    struct B {
        s: String,
    }

    impl<W> WCodec<&B, &mut W> for Zenoh080
    where
        W: Writer,
    {
        type Output = Result<(), ()>;

        fn write(self, writer: &mut W, message: &B) -> Self::Output {
            self.write(&mut *writer, &message.s).map_err(|_| ())?;
            Ok(())
        }
    }

    impl<R> RCodec<B, &mut R> for Zenoh080
    where
        R: Reader,
    {
        type Error = ();

        fn read(self, reader: &mut R) -> Result<B, Self::Error> {
            let s: String = self.read(&mut *reader).map_err(|_| ())?;
            Ok(B { s })
        }
    }

    #[test]
    fn msgidx_cascade() {
        let a = A {
            one: 42,
            b: B {
                s: String::from("test"),
            },
        };

        let mut buffer = vec![];
        let codec = Zenoh080::new();

        let mut writer = buffer.writer();
        codec.write(&mut writer, &a).unwrap();

        let mut reader = buffer.reader();
        let a_out: A = codec.read(&mut reader).unwrap();

        assert_eq!(a, a_out);
    }
}
