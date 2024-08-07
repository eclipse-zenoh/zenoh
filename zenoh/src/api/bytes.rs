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
use std::{
    borrow::Cow, collections::HashMap, convert::Infallible, fmt::Debug, marker::PhantomData,
    str::Utf8Error, string::FromUtf8Error, sync::Arc,
};

use uhlc::Timestamp;
use unwrap_infallible::UnwrapInfallible;
use zenoh_buffers::{
    buffer::{Buffer, SplitBuffer},
    reader::{DidntRead, HasReader, Reader},
    writer::HasWriter,
    ZBuf, ZBufReader, ZBufWriter, ZSlice, ZSliceBuffer,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    core::{Encoding as EncodingProto, Parameters},
    zenoh::ext::AttachmentType,
};
#[cfg(feature = "shared-memory")]
use zenoh_shm::{
    api::buffer::{
        zshm::{zshm, ZShm},
        zshmmut::{zshmmut, ZShmMut},
    },
    ShmBufInner,
};

use super::{encoding::Encoding, value::Value};

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

/// Trait to encode a type `T` into a [`Value`].
pub trait Serialize<T> {
    type Output;

    /// The implementer should take care of serializing the type `T` and set the proper [`Encoding`].
    fn serialize(self, t: T) -> Self::Output;
}

pub trait Deserialize<T> {
    type Input<'a>;
    type Error;

    /// The implementer should take care of deserializing the type `T` based on the [`Encoding`] information.
    fn deserialize(self, t: Self::Input<'_>) -> Result<T, Self::Error>;
}

/// ZBytes contains the serialized bytes of user data.
///
/// `ZBytes` provides convenient methods to the user for serialization/deserialization based on the default Zenoh serializer [`ZSerde`].
///
/// **NOTE:** Zenoh semantic and protocol take care of sending and receiving bytes without restricting the actual data types.
/// [`ZSerde`] is the default serializer/deserializer provided for convenience to the users to deal with primitives data types via
/// a simple out-of-the-box encoding. [`ZSerde`] is **NOT** by any means the only serializer/deserializer users can use nor a limitation
/// to the types supported by Zenoh. Users are free and encouraged to use any serializer/deserializer of their choice like *serde*,
/// *protobuf*, *bincode*, *flatbuffers*, etc.
///
/// `ZBytes` can be used to serialize a single type:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// let start = String::from("abc");
/// let bytes = ZBytes::serialize(start.clone());
/// let end: String = bytes.deserialize().unwrap();
/// assert_eq!(start, end);
/// ```
///
/// A tuple of serializable types:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// let start = (String::from("abc"), String::from("def"));
/// let bytes = ZBytes::serialize(start.clone());
/// let end: (String, String) = bytes.deserialize().unwrap();
/// assert_eq!(start, end);
///
/// let start = (1_u8, 3.14_f32, String::from("abc"));
/// let bytes = ZBytes::serialize(start.clone());
/// let end: (u8, f32, String) = bytes.deserialize().unwrap();
/// assert_eq!(start, end);
/// ``````
///
/// An iterator of serializable types:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// let start = vec![String::from("abc"), String::from("def")];
/// let bytes = ZBytes::from_iter(start.iter());
///
/// let mut i = 0;
/// let mut iter = bytes.iter::<String>();
/// while let Some(Ok(t)) = iter.next() {
///     assert_eq!(start[i], t);
///     i += 1;
/// }
/// ```
///
/// A writer and a reader of serializable types:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// #[derive(Debug, PartialEq)]
/// struct Foo {
///     one: usize,
///     two: String,
///     three: Vec<u8>,
/// }
///
/// let start = Foo {
///     one: 42,
///     two: String::from("Forty-Two"),
///     three: vec![42u8; 42],
/// };
///
/// let mut bytes = ZBytes::empty();
/// let mut writer = bytes.writer();
///
/// writer.serialize(&start.one);
/// writer.serialize(&start.two);
/// writer.serialize(&start.three);
///
/// let mut reader = bytes.reader();
/// let end = Foo {
///     one: reader.deserialize().unwrap(),
///     two: reader.deserialize().unwrap(),
///     three: reader.deserialize().unwrap(),
/// };
/// assert_eq!(start, end);
/// ```
///
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

    /// Returns whether the ZBytes is empty or not.
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
        for<'b> ZSerde: Deserialize<T, Input<'b> = &'b ZBytes>,
        for<'b> <ZSerde as Deserialize<T>>::Error: Debug,
    {
        ZBytesIterator {
            reader: self.0.reader(),
            _t: PhantomData::<T>,
        }
    }

    /// Serialize an object of type `T` as a [`ZBytes`] using the [`ZSerde`].
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
        ZSerde: Serialize<T, Output = Self>,
    {
        ZSerde.serialize(t)
    }

    /// Try serializing an object of type `T` as a [`ZBytes`] using the [`ZSerde`].
    ///
    /// ```rust
    /// use serde_json::Value;
    /// use zenoh::bytes::ZBytes;
    ///
    /// // Some JSON input data as a &str. Maybe this comes from the user.
    /// let data = r#"
    /// {
    ///     "name": "John Doe",
    ///     "age": 43,
    ///     "phones": [
    ///         "+44 1234567",
    ///         "+44 2345678"
    ///     ]
    /// }"#;
    ///
    /// // Parse the string of data into serde_json::Value.
    /// let start: Value = serde_json::from_str(data).unwrap();
    /// // The serialization of a serde_json::Value is faillable (see `serde_json::to_string()`).
    /// let bytes = ZBytes::try_serialize(start.clone()).unwrap();
    /// let end: Value = bytes.deserialize().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn try_serialize<T, E>(t: T) -> Result<Self, E>
    where
        ZSerde: Serialize<T, Output = Result<Self, E>>,
    {
        ZSerde.serialize(t)
    }

    /// Deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn deserialize<'a, T>(&'a self) -> Result<T, <ZSerde as Deserialize<T>>::Error>
    where
        ZSerde: Deserialize<T, Input<'a> = &'a ZBytes>,
        <ZSerde as Deserialize<T>>::Error: Debug,
    {
        ZSerde.deserialize(self)
    }

    /// Deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn deserialize_mut<'a, T>(&'a mut self) -> Result<T, <ZSerde as Deserialize<T>>::Error>
    where
        ZSerde: Deserialize<T, Input<'a> = &'a mut ZBytes>,
        <ZSerde as Deserialize<T>>::Error: Debug,
    {
        ZSerde.deserialize(self)
    }

    /// Infallibly deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn into<'a, T>(&'a self) -> T
    where
        ZSerde: Deserialize<T, Input<'a> = &'a ZBytes, Error = Infallible>,
        <ZSerde as Deserialize<T>>::Error: Debug,
    {
        ZSerde.deserialize(self).unwrap_infallible()
    }

    /// Infallibly deserialize an object of type `T` from a [`Value`] using the [`ZSerde`].
    pub fn into_mut<'a, T>(&'a mut self) -> T
    where
        ZSerde: Deserialize<T, Input<'a> = &'a mut ZBytes, Error = Infallible>,
        <ZSerde as Deserialize<T>>::Error: Debug,
    {
        ZSerde.deserialize(self).unwrap_infallible()
    }
}

/// A reader that implements [`std::io::Read`] trait to read from a [`ZBytes`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesReader<'a>(ZBufReader<'a>);

#[derive(Debug)]
pub enum ZReadOrDeserializeError<T>
where
    T: TryFrom<ZBytes>,
    <T as TryFrom<ZBytes>>::Error: Debug,
{
    Read(DidntRead),
    Deserialize(<T as TryFrom<ZBytes>>::Error),
}

impl<T> std::fmt::Display for ZReadOrDeserializeError<T>
where
    T: Debug,
    T: TryFrom<ZBytes>,
    <T as TryFrom<ZBytes>>::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZReadOrDeserializeError::Read(_) => f.write_str("Read error"),
            ZReadOrDeserializeError::Deserialize(e) => f.write_fmt(format_args!("{:?}", e)),
        }
    }
}

impl<T> std::error::Error for ZReadOrDeserializeError<T>
where
    T: Debug,
    T: TryFrom<ZBytes>,
    <T as TryFrom<ZBytes>>::Error: Debug,
{
}

impl ZBytesReader<'_> {
    /// Returns the number of bytes that can still be read
    pub fn remaining(&self) -> usize {
        self.0.remaining()
    }

    /// Returns true if no more bytes can be read
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Deserialize an object of type `T` from a [`ZBytesReader`] using the [`ZSerde`].
    /// See [`ZBytesWriter::serialize`] for an example.
    pub fn deserialize<T>(&mut self) -> Result<T, <ZSerde as Deserialize<T>>::Error>
    where
        for<'a> ZSerde: Deserialize<T, Input<'a> = &'a ZBytes>,
        <ZSerde as Deserialize<T>>::Error: Debug,
    {
        let codec = Zenoh080::new();
        let abuf: ZBuf = codec.read(&mut self.0).unwrap();
        let apld = ZBytes::new(abuf);

        let a = ZSerde.deserialize(&apld)?;
        Ok(a)
    }
}

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

impl ZBytesWriter<'_> {
    fn write(&mut self, bytes: &ZBuf) {
        let codec = Zenoh080::new();
        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe { codec.write(&mut self.0, bytes).unwrap_unchecked() };
    }

    /// Serialize a type `T` on the [`ZBytes`]. For simmetricity, every serialization
    /// operation preserves type boundaries by preprending the length of the serialized data.
    /// This allows calling [`ZBytesReader::deserialize`] in the same order to retrieve the original type.
    ///
    /// Example:
    /// ```
    /// // serialization
    /// let mut bytes = ZBytes::empty();
    /// let mut writer = bytes.writer();
    /// let i1 = 1234_u32;
    /// let i2 = String::from("test");
    /// let i3 = vec![1, 2, 3, 4];
    /// writer.serialize(i1);
    /// writer.serialize(&i2);
    /// writer.serialize(&i3);
    /// // deserialization
    /// let mut reader = bytes.reader();
    /// let o1: u32 = reader.deserialize().unwrap();
    /// let o2: String = reader.deserialize().unwrap();
    /// let o3: Vec<u8> = reader.deserialize().unwrap();
    /// assert_eq!(i1, o1);
    /// assert_eq!(i2, o2);
    /// assert_eq!(i3, o3);
    /// ```
    pub fn serialize<T>(&mut self, t: T)
    where
        ZSerde: Serialize<T, Output = ZBytes>,
    {
        let tpld = ZSerde.serialize(t);
        self.write(&tpld.0);
    }

    /// Try to serialize a type `T` on the [`ZBytes`]. Serialization works
    /// in the same way as [`ZBytesWriter::serialize`].
    pub fn try_serialize<T, E>(&mut self, t: T) -> Result<(), E>
    where
        ZSerde: Serialize<T, Output = Result<ZBytes, E>>,
    {
        let tpld = ZSerde.serialize(t)?;
        self.write(&tpld.0);
        Ok(())
    }

    /// Append a [`ZBytes`] to this [`ZBytes`] by taking ownership.
    /// This allows to compose a [`ZBytes`] out of multiple [`ZBytes`] that may point to different memory regions.
    /// Said in other terms, it allows to create a linear view on different memory regions without copy.
    /// Please note that `append` does not preserve any boundaries as done in [`ZBytesWriter::serialize`], meaning
    /// that [`ZBytesReader::deserialize`] will not be able to deserialize the types in the same seriliazation order.
    /// You will need to decide how to deserialize data yourself.
    ///
    /// Example:
    /// ```
    /// let one = ZBytes::from(vec![0, 1]);
    /// let two = ZBytes::from(vec![2, 3, 4, 5]);
    /// let three = ZBytes::from(vec![6, 7]);
    ///
    /// let mut bytes = ZBytes::empty();
    /// let mut writer = bytes.writer();
    /// // Append data without copying by passing ownership
    /// writer.append(one);
    /// writer.append(two);
    /// writer.append(three);
    ///
    /// // deserialization
    /// let mut out = bytes.into::<Vec<[u8]>>();
    /// assert_eq!(out, vec![0u8, 1, 2, 3, 4, 5, 6, 7]);
    /// ```
    pub fn append(&mut self, b: ZBytes) {
        use zenoh_buffers::writer::Writer;
        for s in b.0.zslices() {
            // SAFETY: we are writing a ZSlice on a ZBuf, this is infallible because we are just pushing a ZSlice to
            //         the list of available ZSlices.
            unsafe { self.0.write_zslice(s).unwrap_unchecked() }
        }
    }
}

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
pub struct ZBytesIterator<'a, T> {
    reader: ZBufReader<'a>,
    _t: PhantomData<T>,
}

impl<T> Iterator for ZBytesIterator<'_, T>
where
    for<'a> ZSerde: Deserialize<T, Input<'a> = &'a ZBytes>,
    <ZSerde as Deserialize<T>>::Error: Debug,
{
    type Item = Result<T, <ZSerde as Deserialize<T>>::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let codec = Zenoh080::new();

        let kbuf: ZBuf = codec.read(&mut self.reader).ok()?;
        let kpld = ZBytes::new(kbuf);

        Some(ZSerde.deserialize(&kpld))
    }
}

impl<A> FromIterator<A> for ZBytes
where
    ZSerde: Serialize<A, Output = ZBytes>,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut bytes = ZBytes::empty();
        let mut writer = bytes.writer();
        for t in iter {
            writer.serialize(t);
        }

        ZBytes::new(bytes)
    }
}

/// The default serializer for [`ZBytes`]. It supports primitives types, such as: `Vec<u8>`, `uX`, `iX`, `fX`, `String`, `bool`.
/// It also supports common Rust serde values like `serde_json::Value`.
///
/// **NOTE:** Zenoh semantic and protocol take care of sending and receiving bytes without restricting the actual data types.
/// [`ZSerde`] is the default serializer/deserializer provided for convenience to the users to deal with primitives data types via
/// a simple out-of-the-box encoding. [`ZSerde`] is **NOT** by any means the only serializer/deserializer users can use nor a limitation
/// to the types supported by Zenoh. Users are free and encouraged to use any serializer/deserializer of their choice like *serde*,
/// *protobuf*, *bincode*, *flatbuffers*, etc.
#[derive(Clone, Copy, Debug)]
pub struct ZSerde;

#[derive(Debug, Clone, Copy)]
pub struct ZDeserializeError;

impl std::fmt::Display for ZDeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Deserialize error")
    }
}

impl std::error::Error for ZDeserializeError {}

// ZBytes
impl Serialize<ZBytes> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: ZBytes) -> Self::Output {
        t
    }
}

impl From<&ZBytes> for ZBytes {
    fn from(t: &ZBytes) -> Self {
        ZSerde.serialize(t)
    }
}

impl From<&mut ZBytes> for ZBytes {
    fn from(t: &mut ZBytes) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&ZBytes> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &ZBytes) -> Self::Output {
        t.clone()
    }
}

impl Serialize<&mut ZBytes> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut ZBytes) -> Self::Output {
        t.clone()
    }
}

impl Deserialize<ZBytes> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'_>) -> Result<ZBytes, Self::Error> {
        Ok(v.clone())
    }
}

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

impl Serialize<&mut ZBuf> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut ZBuf) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&mut ZBuf> for ZBytes {
    fn from(t: &mut ZBuf) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<ZBuf> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'_>) -> Result<ZBuf, Self::Error> {
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

impl From<&mut ZBytes> for ZBuf {
    fn from(value: &mut ZBytes) -> Self {
        ZSerde.deserialize(&*value).unwrap_infallible()
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

impl Serialize<&mut ZSlice> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut ZSlice) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&mut ZSlice> for ZBytes {
    fn from(t: &mut ZSlice) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<ZSlice> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'_>) -> Result<ZSlice, Self::Error> {
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

impl From<&mut ZBytes> for ZSlice {
    fn from(value: &mut ZBytes) -> Self {
        ZSerde.deserialize(&*value).unwrap_infallible()
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

impl<const N: usize> Serialize<&mut [u8; N]> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut [u8; N]) -> Self::Output {
        ZBytes::new(*t)
    }
}

impl<const N: usize> From<&mut [u8; N]> for ZBytes {
    fn from(t: &mut [u8; N]) -> Self {
        ZSerde.serialize(*t)
    }
}

impl<const N: usize> Deserialize<[u8; N]> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'_>) -> Result<[u8; N], Self::Error> {
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

impl<const N: usize> TryFrom<&mut ZBytes> for [u8; N] {
    type Error = ZDeserializeError;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
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

impl Serialize<&mut Vec<u8>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut Vec<u8>) -> Self::Output {
        ZBytes::new(t.clone())
    }
}

impl From<&mut Vec<u8>> for ZBytes {
    fn from(t: &mut Vec<u8>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<Vec<u8>> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'_>) -> Result<Vec<u8>, Self::Error> {
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

impl From<&mut ZBytes> for Vec<u8> {
    fn from(value: &mut ZBytes) -> Self {
        ZSerde.deserialize(&*value).unwrap_infallible()
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

impl Serialize<&mut [u8]> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut [u8]) -> Self::Output {
        ZSerde.serialize(&*t)
    }
}

impl From<&mut [u8]> for ZBytes {
    fn from(t: &mut [u8]) -> Self {
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

impl<'a> Serialize<&mut Cow<'a, [u8]>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut Cow<'a, [u8]>) -> Self::Output {
        ZSerde.serialize(&*t)
    }
}

impl From<&mut Cow<'_, [u8]>> for ZBytes {
    fn from(t: &mut Cow<'_, [u8]>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Deserialize<Cow<'a, [u8]>> for ZSerde {
    type Input<'b> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'a>) -> Result<Cow<'a, [u8]>, Self::Error> {
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

impl<'a> From<&'a mut ZBytes> for Cow<'a, [u8]> {
    fn from(value: &'a mut ZBytes) -> Self {
        ZSerde.deserialize(&*value).unwrap_infallible()
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

impl Serialize<&mut String> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut String) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut String> for ZBytes {
    fn from(t: &mut String) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<String> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = FromUtf8Error;

    fn deserialize(self, v: Self::Input<'_>) -> Result<String, Self::Error> {
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

impl TryFrom<&mut ZBytes> for String {
    type Error = FromUtf8Error;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// &str
impl Serialize<&str> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &str) -> Self::Output {
        ZSerde.serialize(s.to_string())
    }
}

impl From<&str> for ZBytes {
    fn from(t: &str) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut str> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut str) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut str> for ZBytes {
    fn from(t: &mut str) -> Self {
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
        ZSerde.serialize(s.to_string())
    }
}

impl From<&Cow<'_, str>> for ZBytes {
    fn from(t: &Cow<'_, str>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Serialize<&mut Cow<'a, str>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut Cow<'a, str>) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut Cow<'_, str>> for ZBytes {
    fn from(t: &mut Cow<'_, str>) -> Self {
        ZSerde.serialize(t)
    }
}

impl<'a> Deserialize<Cow<'a, str>> for ZSerde {
    type Input<'b> = &'a ZBytes;
    type Error = Utf8Error;

    fn deserialize(self, v: Self::Input<'a>) -> Result<Cow<'a, str>, Self::Error> {
        Cow::try_from(v)
    }
}

impl TryFrom<ZBytes> for Cow<'static, str> {
    type Error = Utf8Error;

    fn try_from(v: ZBytes) -> Result<Self, Self::Error> {
        Ok(match Cow::<[u8]>::from(v) {
            Cow::Borrowed(s) => core::str::from_utf8(s)?.into(),
            Cow::Owned(s) => String::from_utf8(s).map_err(|err| err.utf8_error())?.into(),
        })
    }
}

impl<'a> TryFrom<&'a ZBytes> for Cow<'a, str> {
    type Error = Utf8Error;

    fn try_from(v: &'a ZBytes) -> Result<Self, Self::Error> {
        Ok(match Cow::<[u8]>::from(v) {
            Cow::Borrowed(s) => core::str::from_utf8(s)?.into(),
            Cow::Owned(s) => String::from_utf8(s).map_err(|err| err.utf8_error())?.into(),
        })
    }
}

impl<'a> TryFrom<&'a mut ZBytes> for Cow<'a, str> {
    type Error = Utf8Error;

    fn try_from(v: &'a mut ZBytes) -> Result<Self, Self::Error> {
        Ok(match Cow::<[u8]>::from(v) {
            Cow::Borrowed(s) => core::str::from_utf8(s)?.into(),
            Cow::Owned(s) => String::from_utf8(s).map_err(|err| err.utf8_error())?.into(),
        })
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
                ZBytes::new(unsafe { ZSlice::new(Arc::new(bs), 0, end).unwrap_unchecked() })
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

        impl Serialize<&mut $t> for ZSerde {
            type Output = ZBytes;

            fn serialize(self, t: &mut $t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl From<&mut $t> for ZBytes {
            fn from(t: &mut $t) -> Self {
                ZSerde.serialize(t)
            }
        }

        impl Deserialize<$t> for ZSerde {
            type Input<'a> = &'a ZBytes;
            type Error = ZDeserializeError;

            fn deserialize(self, v: Self::Input<'_>) -> Result<$t, Self::Error> {
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

        impl TryFrom<&mut ZBytes> for $t {
            type Error = ZDeserializeError;

            fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
                ZSerde.deserialize(&*value)
            }
        }
    };
}

// Zenoh unsigned integers
impl_int!(u8);
impl_int!(u16);
impl_int!(u32);
impl_int!(u64);
impl_int!(u128);
impl_int!(usize);

// Zenoh signed integers
impl_int!(i8);
impl_int!(i16);
impl_int!(i32);
impl_int!(i64);
impl_int!(i128);
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

impl Serialize<&mut bool> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut bool) -> Self::Output {
        ZSerde.serialize(*t)
    }
}

impl From<&mut bool> for ZBytes {
    fn from(t: &mut bool) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<bool> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'_>) -> Result<bool, Self::Error> {
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

impl TryFrom<&mut ZBytes> for bool {
    type Error = ZDeserializeError;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Zenoh char
impl Serialize<char> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: char) -> Self::Output {
        // We can convert char to u32 and encode it as such
        // See https://doc.rust-lang.org/std/primitive.char.html#method.from_u32
        ZSerde.serialize(t as u32)
    }
}

impl From<char> for ZBytes {
    fn from(t: char) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&char> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &char) -> Self::Output {
        ZSerde.serialize(*t)
    }
}

impl From<&char> for ZBytes {
    fn from(t: &char) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut char> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut char) -> Self::Output {
        ZSerde.serialize(*t)
    }
}

impl From<&mut char> for ZBytes {
    fn from(t: &mut char) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<char> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'_>) -> Result<char, Self::Error> {
        let c = v.deserialize::<u32>()?;
        let c = char::try_from(c).map_err(|_| ZDeserializeError)?;
        Ok(c)
    }
}

impl TryFrom<ZBytes> for char {
    type Error = ZDeserializeError;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for char {
    type Error = ZDeserializeError;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<&mut ZBytes> for char {
    type Error = ZDeserializeError;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// - Zenoh advanced types serializer/deserializer
// Parameters
impl Serialize<Parameters<'_>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: Parameters<'_>) -> Self::Output {
        Self.serialize(t.as_str())
    }
}

impl From<Parameters<'_>> for ZBytes {
    fn from(t: Parameters<'_>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Parameters<'_>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &Parameters<'_>) -> Self::Output {
        Self.serialize(t.as_str())
    }
}

impl<'s> From<&'s Parameters<'s>> for ZBytes {
    fn from(t: &'s Parameters<'s>) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut Parameters<'_>> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: &mut Parameters<'_>) -> Self::Output {
        Self.serialize(t.as_str())
    }
}

impl<'s> From<&'s mut Parameters<'s>> for ZBytes {
    fn from(t: &'s mut Parameters<'s>) -> Self {
        ZSerde.serialize(&*t)
    }
}

impl<'a> Deserialize<Parameters<'a>> for ZSerde {
    type Input<'b> = &'a ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'a>) -> Result<Parameters<'a>, Self::Error> {
        let s = v
            .deserialize::<Cow<'a, str>>()
            .map_err(|_| ZDeserializeError)?;
        Ok(Parameters::from(s))
    }
}

impl TryFrom<ZBytes> for Parameters<'static> {
    type Error = ZDeserializeError;

    fn try_from(v: ZBytes) -> Result<Self, Self::Error> {
        let s = v.deserialize::<Cow<str>>().map_err(|_| ZDeserializeError)?;
        Ok(Parameters::from(s.into_owned()))
    }
}

impl<'s> TryFrom<&'s ZBytes> for Parameters<'s> {
    type Error = ZDeserializeError;

    fn try_from(value: &'s ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl<'s> TryFrom<&'s mut ZBytes> for Parameters<'s> {
    type Error = ZDeserializeError;

    fn try_from(value: &'s mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Timestamp
impl Serialize<Timestamp> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: Timestamp) -> Self::Output {
        ZSerde.serialize(&s)
    }
}

impl From<Timestamp> for ZBytes {
    fn from(t: Timestamp) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Timestamp> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &Timestamp) -> Self::Output {
        let codec = Zenoh080::new();
        let mut buffer = ZBuf::empty();
        let mut writer = buffer.writer();
        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe {
            codec.write(&mut writer, s).unwrap_unchecked();
        }
        ZBytes::from(buffer)
    }
}

impl From<&Timestamp> for ZBytes {
    fn from(t: &Timestamp) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut Timestamp> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut Timestamp) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut Timestamp> for ZBytes {
    fn from(t: &mut Timestamp) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<Timestamp> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = zenoh_buffers::reader::DidntRead;

    fn deserialize(self, v: Self::Input<'_>) -> Result<Timestamp, Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = v.0.reader();
        let e: Timestamp = codec.read(&mut reader)?;
        Ok(e)
    }
}

impl TryFrom<ZBytes> for Timestamp {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for Timestamp {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<&mut ZBytes> for Timestamp {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Encoding
impl Serialize<Encoding> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: Encoding) -> Self::Output {
        let e: EncodingProto = s.into();
        let codec = Zenoh080::new();
        let mut buffer = ZBuf::empty();
        let mut writer = buffer.writer();
        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe {
            codec.write(&mut writer, &e).unwrap_unchecked();
        }
        ZBytes::from(buffer)
    }
}

impl From<Encoding> for ZBytes {
    fn from(t: Encoding) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Encoding> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &Encoding) -> Self::Output {
        ZSerde.serialize(s.clone())
    }
}

impl From<&Encoding> for ZBytes {
    fn from(t: &Encoding) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut Encoding> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut Encoding) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut Encoding> for ZBytes {
    fn from(t: &mut Encoding) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<Encoding> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = zenoh_buffers::reader::DidntRead;

    fn deserialize(self, v: Self::Input<'_>) -> Result<Encoding, Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = v.0.reader();
        let e: EncodingProto = codec.read(&mut reader)?;
        Ok(e.into())
    }
}

impl TryFrom<ZBytes> for Encoding {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for Encoding {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<&mut ZBytes> for Encoding {
    type Error = zenoh_buffers::reader::DidntRead;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Value
impl Serialize<Value> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: Value) -> Self::Output {
        ZSerde.serialize((s.payload(), s.encoding()))
    }
}

impl From<Value> for ZBytes {
    fn from(t: Value) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&Value> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &Value) -> Self::Output {
        ZSerde.serialize(s.clone())
    }
}

impl From<&Value> for ZBytes {
    fn from(t: &Value) -> Self {
        ZSerde.serialize(t)
    }
}

impl Serialize<&mut Value> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: &mut Value) -> Self::Output {
        ZSerde.serialize(&*s)
    }
}

impl From<&mut Value> for ZBytes {
    fn from(t: &mut Value) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<Value> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = ZReadOrDeserializeErrorTuple2<ZBytes, Encoding>;

    fn deserialize(self, v: Self::Input<'_>) -> Result<Value, Self::Error> {
        let (payload, encoding) = v.deserialize::<(ZBytes, Encoding)>()?;
        Ok(Value::new(payload, encoding))
    }
}

impl TryFrom<ZBytes> for Value {
    type Error = ZReadOrDeserializeErrorTuple2<ZBytes, Encoding>;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for Value {
    type Error = ZReadOrDeserializeErrorTuple2<ZBytes, Encoding>;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<&mut ZBytes> for Value {
    type Error = ZReadOrDeserializeErrorTuple2<ZBytes, Encoding>;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
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

impl Serialize<&mut serde_json::Value> for ZSerde {
    type Output = Result<ZBytes, serde_json::Error>;

    fn serialize(self, t: &mut serde_json::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_json::to_writer(bytes.writer(), t)?;
        Ok(bytes)
    }
}

impl TryFrom<&mut serde_json::Value> for ZBytes {
    type Error = serde_json::Error;

    fn try_from(value: &mut serde_json::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(&*value)
    }
}

impl Deserialize<serde_json::Value> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = serde_json::Error;

    fn deserialize(self, v: Self::Input<'_>) -> Result<serde_json::Value, Self::Error> {
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

impl TryFrom<&mut ZBytes> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
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

impl Serialize<&mut serde_yaml::Value> for ZSerde {
    type Output = Result<ZBytes, serde_yaml::Error>;

    fn serialize(self, t: &mut serde_yaml::Value) -> Self::Output {
        let mut bytes = ZBytes::empty();
        serde_yaml::to_writer(bytes.writer(), t)?;
        Ok(bytes)
    }
}

impl TryFrom<&mut serde_yaml::Value> for ZBytes {
    type Error = serde_yaml::Error;

    fn try_from(value: &mut serde_yaml::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<serde_yaml::Value> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = serde_yaml::Error;

    fn deserialize(self, v: Self::Input<'_>) -> Result<serde_yaml::Value, Self::Error> {
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

impl TryFrom<&mut ZBytes> for serde_yaml::Value {
    type Error = serde_yaml::Error;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
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

impl Serialize<&mut serde_cbor::Value> for ZSerde {
    type Output = Result<ZBytes, serde_cbor::Error>;

    fn serialize(self, t: &mut serde_cbor::Value) -> Self::Output {
        ZSerde.serialize(&*t)
    }
}

impl TryFrom<&mut serde_cbor::Value> for ZBytes {
    type Error = serde_cbor::Error;

    fn try_from(value: &mut serde_cbor::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<serde_cbor::Value> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = serde_cbor::Error;

    fn deserialize(self, v: Self::Input<'_>) -> Result<serde_cbor::Value, Self::Error> {
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

impl TryFrom<&mut ZBytes> for serde_cbor::Value {
    type Error = serde_cbor::Error;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
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

impl Serialize<&mut serde_pickle::Value> for ZSerde {
    type Output = Result<ZBytes, serde_pickle::Error>;

    fn serialize(self, t: &mut serde_pickle::Value) -> Self::Output {
        ZSerde.serialize(&*t)
    }
}

impl TryFrom<&mut serde_pickle::Value> for ZBytes {
    type Error = serde_pickle::Error;

    fn try_from(value: &mut serde_pickle::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

impl Deserialize<serde_pickle::Value> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = serde_pickle::Error;

    fn deserialize(self, v: Self::Input<'_>) -> Result<serde_pickle::Value, Self::Error> {
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

impl TryFrom<&mut ZBytes> for serde_pickle::Value {
    type Error = serde_pickle::Error;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// bytes::Bytes

// Define a transparent wrapper type to get around Rust's orphan rule.
// This allows to use bytes::Bytes directly as supporting buffer of a
// ZSlice resulting in zero-copy and zero-alloc bytes::Bytes serialization.
#[repr(transparent)]
#[derive(Debug)]
struct BytesWrap(bytes::Bytes);

impl ZSliceBuffer for BytesWrap {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl Serialize<bytes::Bytes> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, s: bytes::Bytes) -> Self::Output {
        ZBytes::new(BytesWrap(s))
    }
}

impl From<bytes::Bytes> for ZBytes {
    fn from(t: bytes::Bytes) -> Self {
        ZSerde.serialize(t)
    }
}

impl Deserialize<bytes::Bytes> for ZSerde {
    type Input<'a> = &'a ZBytes;
    type Error = Infallible;

    fn deserialize(self, v: Self::Input<'_>) -> Result<bytes::Bytes, Self::Error> {
        // bytes::Bytes can be constructed only by passing ownership to the constructor.
        // Thereofore, here we are forced to allocate a vector and copy the whole ZBytes
        // content since bytes::Bytes does not support anything else than Box<u8> (and its
        // variants like Vec<u8> and String).
        let v: Vec<u8> = ZSerde.deserialize(v).unwrap_infallible();
        Ok(bytes::Bytes::from(v))
    }
}

impl TryFrom<ZBytes> for bytes::Bytes {
    type Error = Infallible;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl TryFrom<&ZBytes> for bytes::Bytes {
    type Error = Infallible;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<&mut ZBytes> for bytes::Bytes {
    type Error = Infallible;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Serialize<ZShm> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: ZShm) -> Self::Output {
        let slice: ZSlice = t.into();
        ZBytes::new(slice)
    }
}

#[cfg(feature = "shared-memory")]
impl From<ZShm> for ZBytes {
    fn from(t: ZShm) -> Self {
        ZSerde.serialize(t)
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Serialize<ZShmMut> for ZSerde {
    type Output = ZBytes;

    fn serialize(self, t: ZShmMut) -> Self::Output {
        let slice: ZSlice = t.into();
        ZBytes::new(slice)
    }
}

#[cfg(feature = "shared-memory")]
impl From<ZShmMut> for ZBytes {
    fn from(t: ZShmMut) -> Self {
        ZSerde.serialize(t)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> Deserialize<&'a zshm> for ZSerde {
    type Input<'b> = &'a ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'a>) -> Result<&'a zshm, Self::Error> {
        // A ZShm is expected to have only one slice
        let mut zslices = v.0.zslices();
        if let Some(zs) = zslices.next() {
            if let Some(shmb) = zs.downcast_ref::<ShmBufInner>() {
                return Ok(shmb.into());
            }
        }
        Err(ZDeserializeError)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> TryFrom<&'a ZBytes> for &'a zshm {
    type Error = ZDeserializeError;

    fn try_from(value: &'a ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> TryFrom<&'a mut ZBytes> for &'a mut zshm {
    type Error = ZDeserializeError;

    fn try_from(value: &'a mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> Deserialize<&'a mut zshm> for ZSerde {
    type Input<'b> = &'a mut ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'a>) -> Result<&'a mut zshm, Self::Error> {
        // A ZSliceShmBorrowMut is expected to have only one slice
        let mut zslices = v.0.zslices_mut();
        if let Some(zs) = zslices.next() {
            // SAFETY: ShmBufInner cannot change the size of the slice
            if let Some(shmb) = unsafe { zs.downcast_mut::<ShmBufInner>() } {
                return Ok(shmb.into());
            }
        }
        Err(ZDeserializeError)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> Deserialize<&'a mut zshmmut> for ZSerde {
    type Input<'b> = &'a mut ZBytes;
    type Error = ZDeserializeError;

    fn deserialize(self, v: Self::Input<'a>) -> Result<&'a mut zshmmut, Self::Error> {
        // A ZSliceShmBorrowMut is expected to have only one slice
        let mut zslices = v.0.zslices_mut();
        if let Some(zs) = zslices.next() {
            // SAFETY: ShmBufInner cannot change the size of the slice
            if let Some(shmb) = unsafe { zs.downcast_mut::<ShmBufInner>() } {
                return shmb.try_into().map_err(|_| ZDeserializeError);
            }
        }
        Err(ZDeserializeError)
    }
}

#[cfg(feature = "shared-memory")]
impl<'a> TryFrom<&'a mut ZBytes> for &'a mut zshmmut {
    type Error = ZDeserializeError;

    fn try_from(value: &'a mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// Tuple (a, b)
macro_rules! impl_tuple2 {
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
        impl_tuple2!(t)
    }
}

impl<A, B> Serialize<&(A, B)> for ZSerde
where
    for<'a> &'a A: Into<ZBytes>,
    for<'b> &'b B: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: &(A, B)) -> Self::Output {
        impl_tuple2!(t)
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

#[derive(Debug)]
pub enum ZReadOrDeserializeErrorTuple2<A, B>
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    One(ZReadOrDeserializeError<A>),
    Two(ZReadOrDeserializeError<B>),
}

impl<A, B> std::fmt::Display for ZReadOrDeserializeErrorTuple2<A, B>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZReadOrDeserializeErrorTuple2::One(e) => {
                f.write_fmt(format_args!("1st tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple2::Two(e) => {
                f.write_fmt(format_args!("2nd tuple element: {}", e))
            }
        }
    }
}

impl<A, B> std::error::Error for ZReadOrDeserializeErrorTuple2<A, B>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
}

impl<A, B> Deserialize<(A, B)> for ZSerde
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Input<'a> = &'a ZBytes;
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn deserialize(self, bytes: Self::Input<'_>) -> Result<(A, B), Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = bytes.0.reader();

        let abuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple2::One(ZReadOrDeserializeError::Read(e)))?;
        let apld = ZBytes::new(abuf);
        let a = A::try_from(apld).map_err(|e| {
            ZReadOrDeserializeErrorTuple2::One(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let bbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple2::Two(ZReadOrDeserializeError::Read(e)))?;
        let bpld = ZBytes::new(bbuf);
        let b = B::try_from(bpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple2::Two(ZReadOrDeserializeError::Deserialize(e))
        })?;

        Ok((a, b))
    }
}

impl<A, B> TryFrom<ZBytes> for (A, B)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<A, B> TryFrom<&ZBytes> for (A, B)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl<A, B> TryFrom<&mut ZBytes> for (A, B)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Tuple (a, b, c)
macro_rules! impl_tuple3 {
    ($t:expr) => {{
        let (a, b, c) = $t;

        let codec = Zenoh080::new();
        let mut buffer: ZBuf = ZBuf::empty();
        let mut writer = buffer.writer();
        let apld: ZBytes = a.into();
        let bpld: ZBytes = b.into();
        let cpld: ZBytes = c.into();

        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe {
            codec.write(&mut writer, &apld.0).unwrap_unchecked();
            codec.write(&mut writer, &bpld.0).unwrap_unchecked();
            codec.write(&mut writer, &cpld.0).unwrap_unchecked();
        }

        ZBytes::new(buffer)
    }};
}

impl<A, B, C> Serialize<(A, B, C)> for ZSerde
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
    C: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: (A, B, C)) -> Self::Output {
        impl_tuple3!(t)
    }
}

impl<A, B, C> Serialize<&(A, B, C)> for ZSerde
where
    for<'a> &'a A: Into<ZBytes>,
    for<'b> &'b B: Into<ZBytes>,
    for<'b> &'b C: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: &(A, B, C)) -> Self::Output {
        impl_tuple3!(t)
    }
}

impl<A, B, C> From<(A, B, C)> for ZBytes
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
    C: Into<ZBytes>,
{
    fn from(value: (A, B, C)) -> Self {
        ZSerde.serialize(value)
    }
}

#[derive(Debug)]
pub enum ZReadOrDeserializeErrorTuple3<A, B, C>
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    One(ZReadOrDeserializeError<A>),
    Two(ZReadOrDeserializeError<B>),
    Three(ZReadOrDeserializeError<C>),
}

impl<A, B, C> std::fmt::Display for ZReadOrDeserializeErrorTuple3<A, B, C>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZReadOrDeserializeErrorTuple3::One(e) => {
                f.write_fmt(format_args!("1st tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple3::Two(e) => {
                f.write_fmt(format_args!("2nd tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple3::Three(e) => {
                f.write_fmt(format_args!("3rd tuple element: {}", e))
            }
        }
    }
}

impl<A, B, C> std::error::Error for ZReadOrDeserializeErrorTuple3<A, B, C>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
}

impl<A, B, C> Deserialize<(A, B, C)> for ZSerde
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    type Input<'a> = &'a ZBytes;
    type Error = ZReadOrDeserializeErrorTuple3<A, B, C>;

    fn deserialize(self, bytes: Self::Input<'_>) -> Result<(A, B, C), Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = bytes.0.reader();

        let abuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple3::One(ZReadOrDeserializeError::Read(e)))?;
        let apld = ZBytes::new(abuf);
        let a = A::try_from(apld).map_err(|e| {
            ZReadOrDeserializeErrorTuple3::One(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let bbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple3::Two(ZReadOrDeserializeError::Read(e)))?;
        let bpld = ZBytes::new(bbuf);
        let b = B::try_from(bpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple3::Two(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let cbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple3::Three(ZReadOrDeserializeError::Read(e)))?;
        let cpld = ZBytes::new(cbuf);
        let c = C::try_from(cpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple3::Three(ZReadOrDeserializeError::Deserialize(e))
        })?;

        Ok((a, b, c))
    }
}

impl<A, B, C> TryFrom<ZBytes> for (A, B, C)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple3<A, B, C>;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<A, B, C> TryFrom<&ZBytes> for (A, B, C)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple3<A, B, C>;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl<A, B, C> TryFrom<&mut ZBytes> for (A, B, C)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple3<A, B, C>;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Tuple (a, b, c, d)
macro_rules! impl_tuple4 {
    ($t:expr) => {{
        let (a, b, c, d) = $t;

        let codec = Zenoh080::new();
        let mut buffer: ZBuf = ZBuf::empty();
        let mut writer = buffer.writer();
        let apld: ZBytes = a.into();
        let bpld: ZBytes = b.into();
        let cpld: ZBytes = c.into();
        let dpld: ZBytes = d.into();

        // SAFETY: we are serializing slices on a ZBuf, so serialization will never
        //         fail unless we run out of memory. In that case, Rust memory allocator
        //         will panic before the serializer has any chance to fail.
        unsafe {
            codec.write(&mut writer, &apld.0).unwrap_unchecked();
            codec.write(&mut writer, &bpld.0).unwrap_unchecked();
            codec.write(&mut writer, &cpld.0).unwrap_unchecked();
            codec.write(&mut writer, &dpld.0).unwrap_unchecked();
        }

        ZBytes::new(buffer)
    }};
}

impl<A, B, C, D> Serialize<(A, B, C, D)> for ZSerde
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
    C: Into<ZBytes>,
    D: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: (A, B, C, D)) -> Self::Output {
        impl_tuple4!(t)
    }
}

impl<A, B, C, D> Serialize<&(A, B, C, D)> for ZSerde
where
    for<'a> &'a A: Into<ZBytes>,
    for<'b> &'b B: Into<ZBytes>,
    for<'b> &'b C: Into<ZBytes>,
    for<'b> &'b D: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: &(A, B, C, D)) -> Self::Output {
        impl_tuple4!(t)
    }
}

impl<A, B, C, D> From<(A, B, C, D)> for ZBytes
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
    C: Into<ZBytes>,
    D: Into<ZBytes>,
{
    fn from(value: (A, B, C, D)) -> Self {
        ZSerde.serialize(value)
    }
}

#[derive(Debug)]
pub enum ZReadOrDeserializeErrorTuple4<A, B, C, D>
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    One(ZReadOrDeserializeError<A>),
    Two(ZReadOrDeserializeError<B>),
    Three(ZReadOrDeserializeError<C>),
    Four(ZReadOrDeserializeError<D>),
}

impl<A, B, C, D> std::fmt::Display for ZReadOrDeserializeErrorTuple4<A, B, C, D>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZReadOrDeserializeErrorTuple4::One(e) => {
                f.write_fmt(format_args!("1st tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple4::Two(e) => {
                f.write_fmt(format_args!("2nd tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple4::Three(e) => {
                f.write_fmt(format_args!("3rd tuple element: {}", e))
            }
            ZReadOrDeserializeErrorTuple4::Four(e) => {
                f.write_fmt(format_args!("4th tuple element: {}", e))
            }
        }
    }
}

impl<A, B, C, D> std::error::Error for ZReadOrDeserializeErrorTuple4<A, B, C, D>
where
    A: Debug,
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
}

impl<A, B, C, D> Deserialize<(A, B, C, D)> for ZSerde
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    type Input<'a> = &'a ZBytes;
    type Error = ZReadOrDeserializeErrorTuple4<A, B, C, D>;

    fn deserialize(self, bytes: Self::Input<'_>) -> Result<(A, B, C, D), Self::Error> {
        let codec = Zenoh080::new();
        let mut reader = bytes.0.reader();

        let abuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple4::One(ZReadOrDeserializeError::Read(e)))?;
        let apld = ZBytes::new(abuf);
        let a = A::try_from(apld).map_err(|e| {
            ZReadOrDeserializeErrorTuple4::One(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let bbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple4::Two(ZReadOrDeserializeError::Read(e)))?;
        let bpld = ZBytes::new(bbuf);
        let b = B::try_from(bpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple4::Two(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let cbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple4::Three(ZReadOrDeserializeError::Read(e)))?;
        let cpld = ZBytes::new(cbuf);
        let c = C::try_from(cpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple4::Three(ZReadOrDeserializeError::Deserialize(e))
        })?;

        let dbuf: ZBuf = codec
            .read(&mut reader)
            .map_err(|e| ZReadOrDeserializeErrorTuple4::Four(ZReadOrDeserializeError::Read(e)))?;
        let dpld = ZBytes::new(dbuf);
        let d = D::try_from(dpld).map_err(|e| {
            ZReadOrDeserializeErrorTuple4::Four(ZReadOrDeserializeError::Deserialize(e))
        })?;

        Ok((a, b, c, d))
    }
}

impl<A, B, C, D> TryFrom<ZBytes> for (A, B, C, D)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple4<A, B, C, D>;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<A, B, C, D> TryFrom<&ZBytes> for (A, B, C, D)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple4<A, B, C, D>;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl<A, B, C, D> TryFrom<&mut ZBytes> for (A, B, C, D)
where
    A: TryFrom<ZBytes>,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes>,
    <B as TryFrom<ZBytes>>::Error: Debug,
    C: TryFrom<ZBytes>,
    <C as TryFrom<ZBytes>>::Error: Debug,
    D: TryFrom<ZBytes>,
    <D as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple4<A, B, C, D>;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// HashMap
impl<A, B> Serialize<HashMap<A, B>> for ZSerde
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, mut t: HashMap<A, B>) -> Self::Output {
        ZBytes::from_iter(t.drain())
    }
}

impl<A, B> Serialize<&HashMap<A, B>> for ZSerde
where
    for<'a> &'a A: Into<ZBytes>,
    for<'b> &'b B: Into<ZBytes>,
{
    type Output = ZBytes;

    fn serialize(self, t: &HashMap<A, B>) -> Self::Output {
        ZBytes::from_iter(t.iter())
    }
}

impl<A, B> From<HashMap<A, B>> for ZBytes
where
    A: Into<ZBytes>,
    B: Into<ZBytes>,
{
    fn from(value: HashMap<A, B>) -> Self {
        ZSerde.serialize(value)
    }
}

impl<A, B> Deserialize<HashMap<A, B>> for ZSerde
where
    A: TryFrom<ZBytes> + Debug + std::cmp::Eq + std::hash::Hash,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes> + Debug,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Input<'a> = &'a ZBytes;
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn deserialize(self, bytes: Self::Input<'_>) -> Result<HashMap<A, B>, Self::Error> {
        let mut hm = HashMap::new();
        for res in bytes.iter::<(A, B)>() {
            let (k, v) = res?;
            hm.insert(k, v);
        }
        Ok(hm)
    }
}

impl<A, B> TryFrom<ZBytes> for HashMap<A, B>
where
    A: TryFrom<ZBytes> + Debug + std::cmp::Eq + std::hash::Hash,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes> + Debug,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<A, B> TryFrom<&ZBytes> for HashMap<A, B>
where
    A: TryFrom<ZBytes> + Debug + std::cmp::Eq + std::hash::Hash,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes> + Debug,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: &ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl<A, B> TryFrom<&mut ZBytes> for HashMap<A, B>
where
    A: TryFrom<ZBytes> + Debug + std::cmp::Eq + std::hash::Hash,
    <A as TryFrom<ZBytes>>::Error: Debug,
    B: TryFrom<ZBytes> + Debug,
    <B as TryFrom<ZBytes>>::Error: Debug,
{
    type Error = ZReadOrDeserializeErrorTuple2<A, B>;

    fn try_from(value: &mut ZBytes) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&*value)
    }
}

// Protocol attachment extension
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
        use std::borrow::Cow;

        use rand::Rng;
        use zenoh_buffers::{ZBuf, ZSlice};
        #[cfg(feature = "shared-memory")]
        use zenoh_core::Wait;
        use zenoh_protocol::core::Parameters;
        #[cfg(feature = "shared-memory")]
        use zenoh_shm::api::{
            buffer::zshm::{zshm, ZShm},
            protocol_implementations::posix::{
                posix_shm_provider_backend::PosixShmProviderBackend, protocol_id::POSIX_PROTOCOL_ID,
            },
            provider::shm_provider::ShmProviderBuilder,
        };

        use super::ZBytes;
        use crate::bytes::{Deserialize, Serialize, ZSerde};

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

        // WARN: test function body produces stack overflow, so I split it into subroutines
        #[inline(never)]
        fn numeric() {
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
        }
        numeric();

        // WARN: test function body produces stack overflow, so I split it into subroutines
        #[inline(never)]
        fn basic() {
            let mut rng = rand::thread_rng();

            // bool
            serialize_deserialize!(bool, true);
            serialize_deserialize!(bool, false);

            // char
            serialize_deserialize!(char, char::MAX);
            serialize_deserialize!(char, rng.gen::<char>());

            let a = 'a';
            let bytes = ZSerde.serialize(a);
            let s: String = ZSerde.deserialize(&bytes).unwrap();
            assert_eq!(a.to_string(), s);

            let a = String::from("a");
            let bytes = ZSerde.serialize(&a);
            let s: char = ZSerde.deserialize(&bytes).unwrap();
            assert_eq!(a, s.to_string());

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
        }
        basic();

        // WARN: test function body produces stack overflow, so I split it into subroutines
        #[inline(never)]
        fn reader_writer() {
            let mut bytes = ZBytes::empty();
            let mut writer = bytes.writer();

            let i1 = 1_u8;
            let i2 = String::from("abcdef");
            let i3 = vec![2u8; 64];

            println!("Write: {:?}", i1);
            writer.serialize(i1);
            println!("Write: {:?}", i2);
            writer.serialize(&i2);
            println!("Write: {:?}", i3);
            writer.serialize(&i3);

            let mut reader = bytes.reader();
            let o1: u8 = reader.deserialize().unwrap();
            println!("Read: {:?}", o1);
            let o2: String = reader.deserialize().unwrap();
            println!("Read: {:?}", o2);
            let o3: Vec<u8> = reader.deserialize().unwrap();
            println!("Read: {:?}", o3);

            println!();

            assert_eq!(i1, o1);
            assert_eq!(i2, o2);
            assert_eq!(i3, o3);
        }
        reader_writer();

        // SHM
        #[cfg(feature = "shared-memory")]
        fn shm() {
            // create an SHM backend...
            let backend = PosixShmProviderBackend::builder()
                .with_size(4096)
                .unwrap()
                .res()
                .unwrap();
            // ...and an SHM provider
            let provider = ShmProviderBuilder::builder()
                .protocol_id::<POSIX_PROTOCOL_ID>()
                .backend(backend)
                .res();

            // Prepare a layout for allocations
            let layout = provider.alloc(1024).into_layout().unwrap();

            // allocate an SHM buffer
            let mutable_shm_buf = layout.alloc().wait().unwrap();

            // convert to immutable SHM buffer
            let immutable_shm_buf: ZShm = mutable_shm_buf.into();

            serialize_deserialize!(&zshm, immutable_shm_buf);
        }
        #[cfg(feature = "shared-memory")]
        shm();

        // Parameters
        serialize_deserialize!(Parameters, Parameters::from(""));
        serialize_deserialize!(Parameters, Parameters::from("a=1;b=2;c3"));

        // Bytes
        serialize_deserialize!(bytes::Bytes, bytes::Bytes::from(vec![1, 2, 3, 4]));
        serialize_deserialize!(bytes::Bytes, bytes::Bytes::from("Hello World"));

        // Tuple
        serialize_deserialize!((usize, usize), (0, 1));
        serialize_deserialize!((usize, String), (0, String::from("a")));
        serialize_deserialize!((String, String), (String::from("a"), String::from("b")));
        serialize_deserialize!(
            (Cow<'static, [u8]>, Cow<'static, [u8]>),
            (Cow::from(vec![0u8; 8]), Cow::from(vec![0u8; 8]))
        );
        serialize_deserialize!(
            (Cow<'static, str>, Cow<'static, str>),
            (Cow::from("a"), Cow::from("b"))
        );

        fn iterator() {
            let v: [usize; 5] = [0, 1, 2, 3, 4];
            println!("Serialize:\t{:?}", v);
            let p = ZBytes::from_iter(v.iter());
            println!("Deserialize:\t{:?}\n", p);
            for (i, t) in p.iter::<usize>().enumerate() {
                assert_eq!(i, t.unwrap());
            }

            let mut v = vec![[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]];
            println!("Serialize:\t{:?}", v);
            let p = ZBytes::from_iter(v.drain(..));
            println!("Deserialize:\t{:?}\n", p);
            let mut iter = p.iter::<[u8; 4]>();
            assert_eq!(iter.next().unwrap().unwrap(), [0, 1, 2, 3]);
            assert_eq!(iter.next().unwrap().unwrap(), [4, 5, 6, 7]);
            assert_eq!(iter.next().unwrap().unwrap(), [8, 9, 10, 11]);
            assert_eq!(iter.next().unwrap().unwrap(), [12, 13, 14, 15]);
            assert!(iter.next().is_none());
        }
        iterator();

        fn hashmap() {
            use std::collections::HashMap;
            let mut hm: HashMap<usize, usize> = HashMap::new();
            hm.insert(0, 0);
            hm.insert(1, 1);
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<usize, usize>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, Vec<u8>> = HashMap::new();
            hm.insert(0, vec![0u8; 8]);
            hm.insert(1, vec![1u8; 16]);
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<usize, Vec<u8>>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, ZSlice> = HashMap::new();
            hm.insert(0, ZSlice::from(vec![0u8; 8]));
            hm.insert(1, ZSlice::from(vec![1u8; 16]));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<usize, ZSlice>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, ZBuf> = HashMap::new();
            hm.insert(0, ZBuf::from(vec![0u8; 8]));
            hm.insert(1, ZBuf::from(vec![1u8; 16]));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<usize, ZBuf>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<String, String> = HashMap::new();
            hm.insert(String::from("0"), String::from("a"));
            hm.insert(String::from("1"), String::from("b"));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<String, String>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<Cow<'static, str>, Cow<'static, str>> = HashMap::new();
            hm.insert(Cow::from("0"), Cow::from("a"));
            hm.insert(Cow::from("1"), Cow::from("b"));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.deserialize::<HashMap<Cow<str>, Cow<str>>>().unwrap();
            assert_eq!(hm, o);
        }
        hashmap();
    }
}
