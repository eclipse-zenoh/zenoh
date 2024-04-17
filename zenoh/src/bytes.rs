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

//! ZBytes primitives.
use crate::buffers::ZBuf;
use std::{
    borrow::Cow, convert::Infallible, fmt::Debug, marker::PhantomData, ops::Deref, str::Utf8Error,
    string::FromUtf8Error, sync::Arc,
};
use unwrap_infallible::UnwrapInfallible;
use zenoh_buffers::{
    buffer::{Buffer, SplitBuffer},
    reader::HasReader,
    writer::HasWriter,
    ZBufReader, ZBufWriter, ZSlice,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{core::Properties, zenoh::ext::AttachmentType};
use zenoh_result::{ZError, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

/// Trait to encode a type `T` into a [`Value`].
pub trait Serialize<T> {
    type Output;

    /// The implementer should take care of serializing the type `T` and set the proper [`Encoding`].
    fn serialize(self, t: T) -> Self::Output;
}

pub trait Deserialize<'a, T> {
    type Error;

    /// The implementer should take care of deserializing the type `T` based on the [`Encoding`] information.
    fn deserialize(self, t: &'a ZBytes) -> Result<T, Self::Error>;
}

/// ZBytes contains the serialized bytes of user data.
#[repr(transparent)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZBytes(ZBuf);

impl ZBytes {
    /// Create an empty ZBytes.
    pub const fn empty() -> Self {
        Self(ZBuf::empty())
    }

    /// Create a [`ZBytes`] from any type `T` that implements [`Into<ZBuf>`].
    pub fn new<T>(t: T) -> Self
    where
        T: Into<ZBuf>,
    {
        Self(t.into())
    }

    /// Returns wether the ZBytes is empty or not.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the length of the ZBytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Get a [`ZBytesReader`] implementing [`std::io::Read`] trait.
    pub fn reader(&self) -> ZBytesReader<'_> {
        ZBytesReader(self.0.reader())
    }

    /// Build a [`ZBytes`] from a generic reader implementing [`std::io::Read`]. This operation copies data from the reader.
    pub fn from_reader<R>(mut reader: R) -> Result<Self, std::io::Error>
    where
        R: std::io::Read,
    {
        let mut buf: Vec<u8> = vec![];
        reader.read_to_end(&mut buf)?;
        Ok(ZBytes::new(buf))
    }

    /// Get a [`ZBytesWriter`] implementing [`std::io::Write`] trait.
    pub fn writer(&mut self) -> ZBytesWriter<'_> {
        ZBytesWriter(self.0.writer())
    }

    /// Get a [`ZBytesReader`] implementing [`std::io::Read`] trait.
    pub fn iter<T>(&self) -> ZBytesIterator<'_, T>
    where
        T: for<'b> TryFrom<&'b ZBytes>,
        for<'b> ZSerde: Deserialize<'b, T>,
        for<'b> <ZSerde as Deserialize<'b, T>>::Error: Debug,
    {
        ZBytesIterator {
            reader: self.0.reader(),
            _t: PhantomData::<T>,
        }
    }

    /// Serialize an object of type `T` as a [`Value`] using the [`ZSerde`].
    ///
    /// ```rust
    /// use zenoh::bytes::ZBytes;
    ///
    /// let start = String::from("abc");
    /// let bytes = ZBytes::serialize(start.clone());
    /// let end: String = bytes.deserialize().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn serialize<T>(t: T) -> Self
    where
        ZSerde: Serialize<T, Output = ZBytes>,
    {
        ZSerde.serialize(t)
    }

    /// Deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn deserialize<'a, T>(&'a self) -> ZResult<T>
    where
        ZSerde: Deserialize<'a, T>,
        <ZSerde as Deserialize<'a, T>>::Error: Debug,
    {
        ZSerde
            .deserialize(self)
            .map_err(|e| zerror!("{:?}", e).into())
    }

    /// Infallibly deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn into<'a, T>(&'a self) -> T
    where
        ZSerde: Deserialize<'a, T, Error = Infallible>,
        <ZSerde as Deserialize<'a, T>>::Error: Debug,
    {
        ZSerde.deserialize(self).unwrap_infallible()
    }
}

/// A reader that implements [`std::io::Read`] trait to read from a [`ZBytes`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesReader<'a>(ZBufReader<'a>);

impl std::io::Read for ZBytesReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.0, buf)
    }
}

impl std::io::Seek for ZBytesReader<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.0, pos)
    }
}

/// A writer that implements [`std::io::Write`] trait to write into a [`ZBytes`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesWriter<'a>(ZBufWriter<'a>);

impl std::io::Write for ZBytesWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut self.0, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// An iterator that implements [`std::iter::Iterator`] trait to iterate on values `T` in a [`ZBytes`].
/// Note that [`ZBytes`] contains a serialized version of `T` and iterating over a [`ZBytes`] performs lazy deserialization.
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesIterator<'a, T>
where
    ZSerde: Deserialize<'a, T>,
{
    reader: ZBufReader<'a>,
    _t: PhantomData<T>,
}

impl<T> Iterator for ZBytesIterator<'_, T>
where
    for<'a> ZSerde: Deserialize<'a, T>,
    for<'a> <ZSerde as Deserialize<'a, T>>::Error: Debug,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let codec = Zenoh080::new();

        let kbuf: ZBuf = codec.read(&mut self.reader).ok()?;
        let kpld = ZBytes::new(kbuf);

        let t = ZSerde.deserialize(&kpld).ok()?;
        Some(t)
    }
}

impl<A> FromIterator<A> for ZBytes
where
    ZSerde: Serialize<A, Output = ZBytes>,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let codec = Zenoh080::new();
        let mut buffer: ZBuf = ZBuf::empty();
        let mut writer = buffer.writer();
        for t in iter {
            let tpld = ZSerde.serialize(t);
            // SAFETY: we are serializing slices on a ZBuf, so serialization will never
            //         fail unless we run out of memory. In that case, Rust memory allocator
            //         will panic before the serializer has any chance to fail.
            unsafe {
                codec.write(&mut writer, &tpld.0).unwrap_unchecked();
            }
        }

        ZBytes::new(buffer)
    }
}

/// Wrapper type for API ergonomicity to allow any type `T` to be converted into `Option<ZBytes>` where `T` implements `Into<ZBytes>`.
#[repr(transparent)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionZBytes(Option<ZBytes>);

impl<T> From<T> for OptionZBytes
where
    T: Into<ZBytes>,
{
    fn from(value: T) -> Self {
        Self(Some(value.into()))
    }
}

impl<T> From<Option<T>> for OptionZBytes
where
    T: Into<ZBytes>,
{
    fn from(mut value: Option<T>) -> Self {
        match value.take() {
            Some(v) => Self(Some(v.into())),
            None => Self(None),
        }
    }
}

impl<T> From<&Option<T>> for OptionZBytes
where
    for<'a> &'a T: Into<ZBytes>,
{
    fn from(value: &Option<T>) -> Self {
        match value.as_ref() {
            Some(v) => Self(Some(v.into())),
            None => Self(None),
        }
    }
}

impl From<OptionZBytes> for Option<ZBytes> {
    fn from(value: OptionZBytes) -> Self {
        value.0
    }
}

/// The default serializer for ZBytes. It supports primitives types, such as: Vec<u8>, int, uint, float, string, bool.
/// It also supports common Rust serde values.
#[derive(Clone, Copy, Debug)]
pub struct ZSerde;

#[derive(Debug, Clone, Copy)]
pub struct ZDeserializeError;

// ZBuf
impl Serialize<ZBuf> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: ZBuf) -> Self::Output {
        ZBytes::new(t)
    }
}

impl From<ZBuf> for ZBytes {
    fn from(t: ZBuf) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&ZBuf> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &ZBuf) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&ZBuf> for ZBytes {
    fn from(t: &ZBuf) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<'_, ZBuf> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &ZBytes) -> Result<ZBuf, Self::Error> {
        Ok(v.0.clone())
    }
}

impl From<ZBytes> for ZBuf {
    fn from(value: ZBytes) -> Self {
        value.0
    }
}

impl From<&ZBytes> for ZBuf {
    fn from(value: &ZBytes) -> Self {
        ZSerde.deserialize(value).unwrap_infallible()
    }
}

// ZSlice
impl Serialize<ZSlice> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: ZSlice) -> Self::Output {
        ZBytes::new(t)
    }
}

impl From<ZSlice> for ZBytes {
    fn from(t: ZSlice) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&ZSlice> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &ZSlice) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&ZSlice> for ZBytes {
    fn from(t: &ZSlice) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<'_, ZSlice> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &ZBytes) -> Result<ZSlice, Self::Error> {
        Ok(v.0.to_zslice())
    }
}

impl From<ZBytes> for ZSlice {
    fn from(value: ZBytes) -> Self {
        ZBuf::from(value).to_zslice()
    }
}

impl From<&ZBytes> for ZSlice {
    fn from(value: &ZBytes) -> Self {
        ZSerde.deserialize(value).unwrap_infallible()
    }
}

// [u8; N]
impl<const N: usize> Serialize<[u8; N]> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: [u8; N]) -> Self::Output {
        ZBytes::new(t)
    }
}

impl<const N: usize> From<[u8; N]> for ZBytes {
    fn from(t: [u8; N]) -> Self {
        ZSerde.serialize(t)
    }
}

impl<const N: usize> Serialize<&[u8; N]> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &[u8; N]) -> Self::Output {
        ZBytes::new(*t)
    }
}

impl<const N: usize> From<&[u8; N]> for ZBytes {
    fn from(t: &[u8; N]) -> Self {
        ZSerde.serialize(t)
    }
}

impl<const N: usize> Deserialize<'_, [u8; N]> for ZSerde {
    type Error = ZDeserializeError;

    fn deserialize(self, v: &ZBytes) -> Result<[u8; N], Self::Error> {
        use std::io::Read;

        if v.0.len() != N {
            return Err(ZDeserializeError);
        }
        let mut dst = [0u8; N];
        let mut reader = v.reader();
        reader.read_exact(&mut dst).map_err(|_| ZDeserializeError)?;
        Ok(dst)
    }
}

impl<const N: usize> TryFrom<ZBytes> for [u8; N] {
    type Error = ZDeserializeError;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<const N: usize> TryFrom<&ZBytes> for [u8; N] {
    type Error = ZDeserializeError;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// Vec<u8>
impl Serialize<Vec<u8>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Vec<u8>) -> Self::Output {
        ZBytes::new(t)
    }
}

impl From<Vec<u8>> for ZBytes {
    fn from(t: Vec<u8>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Vec<u8>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &Vec<u8>) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&Vec<u8>> for ZBytes {
    fn from(t: &Vec<u8>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<'_, Vec<u8>> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &ZBytes) -> Result<Vec<u8>, Self::Error> {
        Ok(v.0.contiguous().to_vec())
    }
}

impl From<ZBytes> for Vec<u8> {
    fn from(value: ZBytes) -> Self {
        ZSerde.deserialize(&value).unwrap_infallible()
    }
}

impl From<&ZBytes> for Vec<u8> {
    fn from(value: &ZBytes) -> Self {
        ZSerde.deserialize(value).unwrap_infallible()
    }
}

// &[u8]
impl Serialize<&[u8]> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &[u8]) -> Self::Output {
        ZBytes::new(t.to_vec())
    }
}

impl From<&[u8]> for ZBytes {
    fn from(t: &[u8]) -> Self {
        ZSerde.serialize(t)
    }
}

// Cow<[u8]>
impl<'a> Serialize<Cow<'a, [u8]>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Cow<'a, [u8]>) -> Self::Output {
        ZBytes::new(t.to_vec())
    }
}

impl From<Cow<'_, [u8]>> for ZBytes {
    fn from(t: Cow<'_, [u8]>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Serialize<&Cow<'a, [u8]>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &Cow<'a, [u8]>) -> Self::Output {
        ZBytes::new(t.to_vec())
    }
}

impl From<&Cow<'_, [u8]>> for ZBytes {
    fn from(t: &Cow<'_, [u8]>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Deserialize<'a, Cow<'a, [u8]>> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &'a ZBytes) -> Result<Cow<'a, [u8]>, Self::Error> {
        Ok(v.0.contiguous())
    }
}

impl From<ZBytes> for Cow<'static, [u8]> {
    fn from(v: ZBytes) -> Self {
        match v.0.contiguous() {
            Cow::Borrowed(s) => Cow::Owned(s.to_vec()),
            Cow::Owned(s) => Cow::Owned(s),
        }
    }
}

impl<'a> From<&'a ZBytes> for Cow<'a, [u8]> {
    fn from(value: &'a ZBytes) -> Self {
        ZSerde.deserialize(value).unwrap_infallible()
    }
}

// String
impl Serialize<String> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: String) -> Self::Output {
        ZBytes::new(s.into_bytes())
    }
}

impl From<String> for ZBytes {
    fn from(t: String) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&String> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &String) -> Self::Output {
        ZBytes::new(s.clone().into_bytes())
    }
}

impl From<&String> for ZBytes {
    fn from(t: &String) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<'_, String> for ZSerde {
    type Error = FromUtf8Error;

    fn deserialize(self, v: &ZBytes) -> Result<String, Self::Error> {
        let v: Vec<u8> = ZSerde.deserialize(v).unwrap_infallible();
        String::from_utf8(v)
    }
}

impl TryFrom<ZBytes> for String {
    type Error = FromUtf8Error;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for String {
    type Error = FromUtf8Error;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// &str
impl Serialize<&str> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &str) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl From<&str> for ZBytes {
    fn from(t: &str) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Serialize<Cow<'a, str>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: Cow<'a, str>) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl From<Cow<'_, str>> for ZBytes {
    fn from(t: Cow<'_, str>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Serialize<&Cow<'a, str>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &Cow<'a, str>) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl From<&Cow<'_, str>> for ZBytes {
    fn from(t: &Cow<'_, str>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Deserialize<'a, Cow<'a, str>> for ZSerde {
    type Error = Utf8Error;

    fn deserialize(self, v: &'a ZBytes) -> Result<Cow<'a, str>, Self::Error> {
        Cow::try_from(v)
    }
}

impl TryFrom<ZBytes> for Cow<'static, str> {
    type Error = Utf8Error;

    fn try_from(v: ZBytes) -> Result<Self, Self::Error> {
        let v: Cow<'static, [u8]> = Cow::from(v);
        let _ = core::str::from_utf8(v.as_ref())?;
        // SAFETY: &str is &[u8] with the guarantee that every char is UTF-8
        //         As implemented internally https://doc.rust-lang.org/std/str/fn.from_utf8_unchecked.html.
        Ok(unsafe { core::mem::transmute(v) })
    }
}

impl<'a> TryFrom<&'a ZBytes> for Cow<'a, str> {
    type Error = Utf8Error;

    fn try_from(v: &'a ZBytes) -> Result<Self, Self::Error> {
        let v: Cow<'a, [u8]> = Cow::from(v);
        let _ = core::str::from_utf8(v.as_ref())?;
        // SAFETY: &str is &[u8] with the guarantee that every char is UTF-8
        //         As implemented internally https://doc.rust-lang.org/std/str/fn.from_utf8_unchecked.html.
        Ok(unsafe { core::mem::transmute(v) })
    }
}

// - Integers impl
macro_rules! impl_int {
    ($t:ty) => {
        impl Serialize<$t> for ZSerde {
            type Output = ZBytes;

            fn serialize(self, t: $t) -> Self::Output {
                let bs = t.to_le_bytes();
                let mut end = 1;
                if t != 0 as $t {
                    end += bs.iter().rposition(|b| *b != 0).unwrap_or(bs.len() - 1);
                };
                // SAFETY:
                // - 0 is a valid start index because bs is guaranteed to always have a length greater or equal than 1
                // - end is a valid end index because is bounded between 0 and bs.len()
                ZBytes::new(unsafe { ZSlice::new_unchecked(Arc::new(bs), 0, end) })
            }
        }

        impl From<$t> for ZBytes {
            fn from(t: $t) -> Self {
                ZSerde.serialize(t)
            }
        }

        impl Serialize<&$t> for ZSerde {
            type Output = ZBytes;

            fn serialize(self, t: &$t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl From<&$t> for ZBytes {
            fn from(t: &$t) -> Self {
                ZSerde.serialize(t)
            }
        }

        impl<'a> Deserialize<'a, $t> for ZSerde {
            type Error = ZDeserializeError;

            fn deserialize(self, v: &ZBytes) -> Result<$t, Self::Error> {
                use std::io::Read;

                let mut r = v.reader();
                let mut bs = (0 as $t).to_le_bytes();
                if v.len() > bs.len() {
                    return Err(ZDeserializeError);
                }
                r.read_exact(&mut bs[..v.len()])
                    .map_err(|_| ZDeserializeError)?;
                let t = <$t>::from_le_bytes(bs);
                Ok(t)
            }
        }

        impl TryFrom<ZBytes> for $t {
            type Error = ZDeserializeError;

            fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
                ZSerde.deserialize(&value)
            }
        }

        impl TryFrom<&ZBytes> for $t {
            type Error = ZDeserializeError;

            fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
                ZSerde.deserialize(value)
            }
        }
    };
}

// Zenoh unsigned integers
impl_int!(u8);
impl_int!(u16);
impl_int!(u32);
impl_int!(u64);
impl_int!(usize);

// Zenoh signed integers
impl_int!(i8);
impl_int!(i16);
impl_int!(i32);
impl_int!(i64);
impl_int!(isize);

// Zenoh floats
impl_int!(f32);
impl_int!(f64);

// Zenoh bool
impl Serialize<bool> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: bool) -> Self::Output {
        // SAFETY: casting a bool into an integer is well-defined behaviour.
        //      0 is false, 1 is true: https://doc.rust-lang.org/std/primitive.bool.html
        ZBytes::new(ZBuf::from((t as u8).to_le_bytes()))
    }
}

impl From<bool> for ZBytes {
    fn from(t: bool) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&bool> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &bool) -> Self::Output {
        ZSerde.serialize(*t)
    }
}

impl From<&bool> for ZBytes {
    fn from(t: &bool) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<'_, bool> for ZSerde {
    type Error = ZDeserializeError;

    fn deserialize(self, v: &ZBytes) -> Result<bool, Self::Error> {
        let p = v.deserialize::<u8>().map_err(|_| ZDeserializeError)?;
        match p {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ZDeserializeError),
        }
    }
}

impl TryFrom<ZBytes> for bool {
    type Error = ZDeserializeError;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for bool {
    type Error = ZDeserializeError;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// - Zenoh advanced types encoders/decoders
// Properties
impl Serialize<Properties<'_>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Properties<'_>) -> Self::Output {
        Self.serialize(t.as_str())
    }
}

impl From<Properties<'_>> for ZBytes {
    fn from(t: Properties<'_>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Properties<'_>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &Properties<'_>) -> Self::Output {
        Self.serialize(t.as_str())
    }
}

impl<'s> From<&'s Properties<'s>> for ZBytes {
    fn from(t: &'s Properties<'s>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'s> Deserialize<'s, Properties<'s>> for ZSerde {
    type Error = ZDeserializeError;

    fn deserialize(self, v: &'s ZBytes) -> Result<Properties<'s>, Self::Error> {
        let s = v
            .deserialize::<Cow<'s, str>>()
            .map_err(|_| ZDeserializeError)?;
        Ok(Properties::from(s))
    }
}

impl TryFrom<ZBytes> for Properties<'static> {
    type Error = ZDeserializeError;

    fn try_from(v: ZBytes) -> Result<Self, Self::Error> {
        let s = v.deserialize::<Cow<str>>().map_err(|_| ZDeserializeError)?;
        Ok(Properties::from(s.into_owned()))
    }
}

impl<'s> TryFrom<&'s ZBytes> for Properties<'s> {
    type Error = ZDeserializeError;

    fn try_from(value: &'s ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// JSON
impl Serialize<serde_json::Value> for ZSerde {
    type Output = Result<ZBytes, serde_json::Error>;

    fn serialize(self, t: serde_json::Value) -> Self::Output {
        ZSerde.serialize(&t)
    }
}

impl TryFrom<serde_json::Value> for ZBytes {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(&value)
    }
}

impl Serialize<&serde_json::Value> for ZSerde {
    type Output = Result<ZBytes, serde_json::Error>;

    fn serialize(self, t: &serde_json::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_json::to_writer(bytes.writer(), t)?;
        Ok(bytes)
    }
}

impl TryFrom<&serde_json::Value> for ZBytes {
    type Error = serde_json::Error;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<'_, serde_json::Value> for ZSerde {
    type Error = serde_json::Error;

    fn deserialize(self, v: &ZBytes) -> Result<serde_json::Value, Self::Error> {
        serde_json::from_reader(v.reader())
    }
}

impl TryFrom<ZBytes> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// Yaml
impl Serialize<serde_yaml::Value> for ZSerde {
    type Output = Result<ZBytes, serde_yaml::Error>;

    fn serialize(self, t: serde_yaml::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl TryFrom<serde_yaml::Value> for ZBytes {
    type Error = serde_yaml::Error;

    fn try_from(value: serde_yaml::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Serialize<&serde_yaml::Value> for ZSerde {
    type Output = Result<ZBytes, serde_yaml::Error>;

    fn serialize(self, t: &serde_yaml::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_yaml::to_writer(bytes.writer(), t)?;
        Ok(bytes)
    }
}

impl TryFrom<&serde_yaml::Value> for ZBytes {
    type Error = serde_yaml::Error;

    fn try_from(value: &serde_yaml::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<'_, serde_yaml::Value> for ZSerde {
    type Error = serde_yaml::Error;

    fn deserialize(self, v: &ZBytes) -> Result<serde_yaml::Value, Self::Error> {
        serde_yaml::from_reader(v.reader())
    }
}

impl TryFrom<ZBytes> for serde_yaml::Value {
    type Error = serde_yaml::Error;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for serde_yaml::Value {
    type Error = serde_yaml::Error;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// CBOR
impl Serialize<serde_cbor::Value> for ZSerde {
    type Output = Result<ZBytes, serde_cbor::Error>;

    fn serialize(self, t: serde_cbor::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl TryFrom<serde_cbor::Value> for ZBytes {
    type Error = serde_cbor::Error;

    fn try_from(value: serde_cbor::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Serialize<&serde_cbor::Value> for ZSerde {
    type Output = Result<ZBytes, serde_cbor::Error>;

    fn serialize(self, t: &serde_cbor::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_cbor::to_writer(bytes.0.writer(), t)?;
        Ok(bytes)
    }
}

impl TryFrom<&serde_cbor::Value> for ZBytes {
    type Error = serde_cbor::Error;

    fn try_from(value: &serde_cbor::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<'_, serde_cbor::Value> for ZSerde {
    type Error = serde_cbor::Error;

    fn deserialize(self, v: &ZBytes) -> Result<serde_cbor::Value, Self::Error> {
        serde_cbor::from_reader(v.reader())
    }
}

impl TryFrom<ZBytes> for serde_cbor::Value {
    type Error = serde_cbor::Error;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for serde_cbor::Value {
    type Error = serde_cbor::Error;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// Pickle
impl Serialize<serde_pickle::Value> for ZSerde {
    type Output = Result<ZBytes, serde_pickle::Error>;

    fn serialize(self, t: serde_pickle::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl TryFrom<serde_pickle::Value> for ZBytes {
    type Error = serde_pickle::Error;

    fn try_from(value: serde_pickle::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Serialize<&serde_pickle::Value> for ZSerde {
    type Output = Result<ZBytes, serde_pickle::Error>;

    fn serialize(self, t: &serde_pickle::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_pickle::value_to_writer(
            &mut bytes.0.writer(),
            t,
            serde_pickle::SerOptions::default(),
        )?;
        Ok(bytes)
    }
}

impl TryFrom<&serde_pickle::Value> for ZBytes {
    type Error = serde_pickle::Error;

    fn try_from(value: &serde_pickle::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<'_, serde_pickle::Value> for ZSerde {
    type Error = serde_pickle::Error;

    fn deserialize(self, v: &ZBytes) -> Result<serde_pickle::Value, Self::Error> {
        serde_pickle::value_from_reader(v.reader(), serde_pickle::DeOptions::default())
    }
}

impl TryFrom<ZBytes> for serde_pickle::Value {
    type Error = serde_pickle::Error;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for serde_pickle::Value {
    type Error = serde_pickle::Error;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Serialize<Arc<SharedMemoryBuf>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Arc<SharedMemoryBuf>) -> Self::Output {
        ZBytes::new(t)
    }
}
#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for ZBytes {
    fn from(t: Arc<SharedMemoryBuf>) -> Self {
        ZSerde.serialize(t)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<Box<SharedMemoryBuf>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Box<SharedMemoryBuf>) -> Self::Output {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self.serialize(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for ZBytes {
    fn from(t: Box<SharedMemoryBuf>) -> Self {
        ZSerde.serialize(t)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<SharedMemoryBuf> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: SharedMemoryBuf) -> Self::Output {
        ZBytes::new(t)
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for ZBytes {
    fn from(t: SharedMemoryBuf) -> Self {
        ZSerde.serialize(t)
    }
}

#[cfg(feature = "shared-memory")]
impl Deserialize<'_, SharedMemoryBuf> for ZSerde {
    type Error = ZDeserializeError;

    fn deserialize(self, v: &ZBytes) -> Result<SharedMemoryBuf, Self::Error> {
        // A SharedMemoryBuf is expected to have only one slice
        let mut zslices = v.0.zslices();
        if let Some(zs) = zslices.next() {
            if let Some(shmb) = zs.downcast_ref::<SharedMemoryBuf>() {
                return Ok(shmb.clone());
            }
        }
        Err(ZDeserializeError)
    }
}

#[cfg(feature = "shared-memory")]
impl TryFrom<ZBytes> for SharedMemoryBuf {
    type Error = ZDeserializeError;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

// Tuple
macro_rules! impl_tuple {
    ($t:expr) => {{
        let (a, b) = $t;

        let codec = Zenoh080::new();
        let mut buffer: ZBuf = ZBuf::empty();
        let mut writer = buffer.writer();
        let apld: ZBytes = a.into();
        let bpld: ZBytes = b.into();

        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe {
            codec.write(&mut writer, &apld.0).unwrap_unchecked();
            codec.write(&mut writer, &bpld.0).unwrap_unchecked();
        }

        ZBytes::new(buffer)
    }};
}
impl<A, B> Serialize<(A, B)> for ZSerde
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: (A, B)) -> Self::Output {
        impl_tuple!(t)
    }
}

impl<A, B> Serialize<&(A, B)> for ZSerde
where
    for<'a> &'a A: Into<ZBytes>,
    for<'b> &'b B: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: &(A, B)) -> Self::Output {
        impl_tuple!(t)
    }
}

impl<A, B> From<(A, B)> for ZBytes
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
{
    fn from(value: (A, B)) -> Self {
        ZSerde.serialize(value)
    }
}

impl<A, B> Deserialize<'_, (A, B)> for ZSerde
where
    for<'a> A: TryFrom<&'a ZBytes>,
    for<'a> <A as TryFrom<&'a ZBytes>>::Error: Debug,
    for<'b> B: TryFrom<&'b ZBytes>,
    for<'b> <B as TryFrom<&'b ZBytes>>::Error: Debug,
{
    type Error = ZError;

    fn deserialize(self, bytes: &ZBytes) -> Result<(A, B), Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = bytes.0.reader();

        let abuf: ZBuf = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;
        let apld = ZBytes::new(abuf);

        let bbuf: ZBuf = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;
        let bpld = ZBytes::new(bbuf);

        let a = A::try_from(&apld).map_err(|e| zerror!("{:?}", e))?;
        let b = B::try_from(&bpld).map_err(|e| zerror!("{:?}", e))?;
        Ok((a, b))
    }
}

impl<A, B> TryFrom<ZBytes> for (A, B)
where
    A: for<'a> TryFrom<&'a ZBytes>,
    for<'a> <A as TryFrom<&'a ZBytes>>::Error: Debug,
    for<'b> B: TryFrom<&'b ZBytes>,
    for<'b> <B as TryFrom<&'b ZBytes>>::Error: Debug,
{
    type Error = ZError;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<A, B> TryFrom<&ZBytes> for (A, B)
where
    for<'a> A: TryFrom<&'a ZBytes>,
    for<'a> <A as TryFrom<&'a ZBytes>>::Error: Debug,
    for<'b> B: TryFrom<&'b ZBytes>,
    for<'b> <B as TryFrom<&'b ZBytes>>::Error: Debug,
{
    type Error = ZError;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// For convenience to always convert a Value in the examples
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringOrBase64 {
    String(String),
    Base64(String),
}

impl StringOrBase64 {
    pub fn into_string(self) -> String {
        match self {
            StringOrBase64::String(s) | StringOrBase64::Base64(s) => s,
        }
    }
}

impl Deref for StringOrBase64 {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::String(s) | Self::Base64(s) => s,
        }
    }
}

impl std::fmt::Display for StringOrBase64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

impl From<&ZBytes> for StringOrBase64 {
    fn from(v: &ZBytes) -> Self {
        use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
        match v.deserialize::<String>() {
            Ok(s) => StringOrBase64::String(s),
            Err(_) => StringOrBase64::Base64(b64_std_engine.encode(v.into::<Vec<u8>>())),
        }
    }
}

impl<const ID: u8> From<ZBytes> for AttachmentType<ID> {
    fn from(this: ZBytes) -> Self {
        AttachmentType {
            buffer: this.into(),
        }
    }
}

impl<const ID: u8> From<AttachmentType<ID>> for ZBytes {
    fn from(this: AttachmentType<ID>) -> Self {
        this.buffer.into()
    }
}

mod tests {
    #[test]
    fn serializer() {
        use super::ZBytes;
        use rand::Rng;
        use std::borrow::Cow;
        use zenoh_buffers::{ZBuf, ZSlice};
        use zenoh_protocol::core::Properties;

        const NUM: usize = 1_000;

        macro_rules! serialize_deserialize {
            ($t:ty, $in:expr) => {
                let i = $in;
                let t = i.clone();
                println!("Serialize:\t{:?}", t);
                let v = ZBytes::serialize(t);
                println!("Deserialize:\t{:?}", v);
                let o: $t = v.deserialize().unwrap();
                assert_eq!(i, o);
                println!("");
            };
        }

        let mut rng = rand::thread_rng();

        // unsigned integer
        serialize_deserialize!(u8, u8::MIN);
        serialize_deserialize!(u16, u16::MIN);
        serialize_deserialize!(u32, u32::MIN);
        serialize_deserialize!(u64, u64::MIN);
        serialize_deserialize!(usize, usize::MIN);

        serialize_deserialize!(u8, u8::MAX);
        serialize_deserialize!(u16, u16::MAX);
        serialize_deserialize!(u32, u32::MAX);
        serialize_deserialize!(u64, u64::MAX);
        serialize_deserialize!(usize, usize::MAX);

        for _ in 0..NUM {
            serialize_deserialize!(u8, rng.gen::<u8>());
            serialize_deserialize!(u16, rng.gen::<u16>());
            serialize_deserialize!(u32, rng.gen::<u32>());
            serialize_deserialize!(u64, rng.gen::<u64>());
            serialize_deserialize!(usize, rng.gen::<usize>());
        }

        // signed integer
        serialize_deserialize!(i8, i8::MIN);
        serialize_deserialize!(i16, i16::MIN);
        serialize_deserialize!(i32, i32::MIN);
        serialize_deserialize!(i64, i64::MIN);
        serialize_deserialize!(isize, isize::MIN);

        serialize_deserialize!(i8, i8::MAX);
        serialize_deserialize!(i16, i16::MAX);
        serialize_deserialize!(i32, i32::MAX);
        serialize_deserialize!(i64, i64::MAX);
        serialize_deserialize!(isize, isize::MAX);

        for _ in 0..NUM {
            serialize_deserialize!(i8, rng.gen::<i8>());
            serialize_deserialize!(i16, rng.gen::<i16>());
            serialize_deserialize!(i32, rng.gen::<i32>());
            serialize_deserialize!(i64, rng.gen::<i64>());
            serialize_deserialize!(isize, rng.gen::<isize>());
        }

        // float
        serialize_deserialize!(f32, f32::MIN);
        serialize_deserialize!(f64, f64::MIN);

        serialize_deserialize!(f32, f32::MAX);
        serialize_deserialize!(f64, f64::MAX);

        for _ in 0..NUM {
            serialize_deserialize!(f32, rng.gen::<f32>());
            serialize_deserialize!(f64, rng.gen::<f64>());
        }

        // String
        serialize_deserialize!(String, "");
        serialize_deserialize!(String, String::from("abcdef"));

        // Cow<str>
        serialize_deserialize!(Cow<str>, Cow::from(""));
        serialize_deserialize!(Cow<str>, Cow::from(String::from("abcdef")));

        // Vec
        serialize_deserialize!(Vec<u8>, vec![0u8; 0]);
        serialize_deserialize!(Vec<u8>, vec![0u8; 64]);

        // Cow<[u8]>
        serialize_deserialize!(Cow<[u8]>, Cow::from(vec![0u8; 0]));
        serialize_deserialize!(Cow<[u8]>, Cow::from(vec![0u8; 64]));

        // ZBuf
        serialize_deserialize!(ZBuf, ZBuf::from(vec![0u8; 0]));
        serialize_deserialize!(ZBuf, ZBuf::from(vec![0u8; 64]));

        // Properties
        serialize_deserialize!(Properties, Properties::from(""));
        serialize_deserialize!(Properties, Properties::from("a=1;b=2;c3"));

        // Tuple
        serialize_deserialize!((usize, usize), (0, 1));
        serialize_deserialize!((usize, String), (0, String::from("a")));
        serialize_deserialize!((String, String), (String::from("a"), String::from("b")));

        // Iterator
        let v: [usize; 5] = [0, 1, 2, 3, 4];
        println!("Serialize:\t{:?}", v);
        let p = ZBytes::from_iter(v.iter());
        println!("Deserialize:\t{:?}\n", p);
        for (i, t) in p.iter::<usize>().enumerate() {
            assert_eq!(i, t);
        }

        let mut v = vec![[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]];
        println!("Serialize:\t{:?}", v);
        let p = ZBytes::from_iter(v.drain(..));
        println!("Deserialize:\t{:?}\n", p);
        let mut iter = p.iter::<[u8; 4]>();
        assert_eq!(iter.next().unwrap(), [0, 1, 2, 3]);
        assert_eq!(iter.next().unwrap(), [4, 5, 6, 7]);
        assert_eq!(iter.next().unwrap(), [8, 9, 10, 11]);
        assert_eq!(iter.next().unwrap(), [12, 13, 14, 15]);
        assert!(iter.next().is_none());

        use std::collections::HashMap;
        let mut hm: HashMap<usize, usize> = HashMap::new();
        hm.insert(0, 0);
        hm.insert(1, 1);
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().drain());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, usize)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<usize, Vec<u8>> = HashMap::new();
        hm.insert(0, vec![0u8; 8]);
        hm.insert(1, vec![1u8; 16]);
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().drain());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, Vec<u8>)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<usize, Vec<u8>> = HashMap::new();
        hm.insert(0, vec![0u8; 8]);
        hm.insert(1, vec![1u8; 16]);
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().drain());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, Vec<u8>)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<usize, ZSlice> = HashMap::new();
        hm.insert(0, ZSlice::from(vec![0u8; 8]));
        hm.insert(1, ZSlice::from(vec![1u8; 16]));
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().drain());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, ZSlice)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<usize, ZBuf> = HashMap::new();
        hm.insert(0, ZBuf::from(vec![0u8; 8]));
        hm.insert(1, ZBuf::from(vec![1u8; 16]));
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().drain());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, ZBuf)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<usize, Vec<u8>> = HashMap::new();
        hm.insert(0, vec![0u8; 8]);
        hm.insert(1, vec![1u8; 16]);
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.clone().iter().map(|(k, v)| (k, Cow::from(v))));
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(usize, Vec<u8>)>());
        assert_eq!(hm, o);

        let mut hm: HashMap<String, String> = HashMap::new();
        hm.insert(String::from("0"), String::from("a"));
        hm.insert(String::from("1"), String::from("b"));
        println!("Serialize:\t{:?}", hm);
        let p = ZBytes::from_iter(hm.iter());
        println!("Deserialize:\t{:?}\n", p);
        let o = HashMap::from_iter(p.iter::<(String, String)>());
        assert_eq!(hm, o);
    }
}
