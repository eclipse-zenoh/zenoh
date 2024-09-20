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
use std::{borrow::Cow, convert::Infallible, fmt::Debug, marker::PhantomData, mem::size_of};

use uhlc::Timestamp;
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
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
use zenoh_shm::{
    api::buffer::{zshm::zshm, zshmmut::zshmmut},
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

pub trait SerializedSize {
    const FIXED_SIZE: Option<usize> = None;
}
impl<T: SerializedSize> SerializedSize for &T {
    const FIXED_SIZE: Option<usize> = T::FIXED_SIZE;
}

pub enum Serialized<'a> {
    Slice(&'a [u8]),
    ZBytes(ZBytes),
}

impl<'a> From<&'a [u8]> for Serialized<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Slice(value)
    }
}

impl From<ZBytes> for Serialized<'_> {
    fn from(value: ZBytes) -> Self {
        Self::ZBytes(value)
    }
}

pub trait Serialize<'a>: Sized + SerializedSize {
    type Error;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error>;
    fn serialize(self) -> Result<Serialized<'a>, Self::Error>;
}

pub trait Deserialize<'a>: Sized + SerializedSize {
    fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError>;
    fn deserialize(zbytes: &'a ZBytes) -> Result<Self, ZDeserializeError>;
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError>;
}

/// ZBytes contains the serialized bytes of user data.
///
/// `ZBytes` provides convenient methods to the user for serialization/deserialization based on the default Zenoh serializer [`ZSerde`].
///
/// **NOTE 1:** Zenoh semantic and protocol take care of sending and receiving bytes without restricting the actual data types.
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
/// let end: String = bytes.try_deserialize().unwrap();
/// assert_eq!(start, end);
/// ```
///
/// A tuple of serializable types:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// let start = (String::from("abc"), String::from("def"));
/// let bytes = ZBytes::serialize(start.clone());
/// let end: (String, String) = bytes.try_deserialize().unwrap();
/// assert_eq!(start, end);
///
/// let start = (1_u8, 3.14_f32, String::from("abc"));
/// let bytes = ZBytes::serialize(start.clone());
/// let end: (u8, f32, String) = bytes.try_deserialize().unwrap();
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
/// let mut bytes = ZBytes::new();
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
/// **NOTE 2:** `ZBytes` may store data in non-contiguous regions of memory.
/// The typical case for `ZBytes` to store data in different memory regions is when data is received fragmented from the network.
/// The user then can decided to use [`ZBytes::try_deserialize`], [`ZBytes::reader`], [`ZBytes::deserialize`], or [`ZBytes::slices`] depending
/// on their needs.
///
/// To directly access raw data as contiguous slice it is preferred to convert `ZBytes` into a [`std::borrow::Cow<[u8]>`].
/// If `ZBytes` contains all the data in a single memory location, this is guaranteed to be zero-copy. This is the common case for small messages.
/// If `ZBytes` contains data scattered in different memory regions, this operation will do an allocation and a copy. This is the common case for large messages.
///
/// Example:
/// ```rust
/// use std::borrow::Cow;
/// use zenoh::bytes::ZBytes;
///
/// let buf: Vec<u8> = vec![0, 1, 2, 3];
/// let bytes = ZBytes::from(buf.clone());
/// let deser: Cow<[u8]> = bytes.into();
/// assert_eq!(buf.as_slice(), deser.as_ref());
/// ```
///
/// It is also possible to iterate over the raw data that may be scattered on different memory regions.
/// Please note that no guarantee is provided on the internal memory layout of [`ZBytes`] nor on how many slices a given [`ZBytes`] will be composed of.
/// The only provided guarantee is on the bytes order that is preserved.
///
/// Example:
/// ```rust
/// use zenoh::bytes::ZBytes;
///
/// let buf: Vec<u8> = vec![0, 1, 2, 3];
/// let bytes = ZBytes::from(buf.clone());
/// for slice in bytes.slices() {
///     println!("{:02x?}", slice);
/// }
/// ```
#[repr(transparent)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZBytes(ZBuf);

impl ZBytes {
    /// Create an empty ZBytes.
    pub const fn new() -> Self {
        Self(ZBuf::empty())
    }

    /// Returns whether the ZBytes is empty or not.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the total number of bytes in the ZBytes.
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
        Ok(ZBytes(buf.into()))
    }

    /// Get a [`ZBytesWriter`] implementing [`std::io::Write`] trait.
    ///
    /// See [`ZBytesWriter`] on how to chain the serialization of different types into a single [`ZBytes`].
    pub fn writer(&mut self) -> ZBytesWriter<'_> {
        ZBytesWriter(self.0.writer())
    }

    /// Get a [`ZBytesIterator`] that deserializes a sequence of `T`.
    ///
    /// Example:
    /// ```rust
    /// use zenoh::bytes::ZBytes;
    ///
    /// let list: Vec<f32> = vec![1.1, 2.2, 3.3];
    /// let mut zbs = ZBytes::from_iter(list.iter());
    ///
    /// for (index, elem) in zbs.iter::<f32>().enumerate() {
    ///     assert_eq!(list[index], elem.unwrap());
    /// }
    /// ```
    pub fn iter<T>(&self) -> ZBytesIterator<'_, T> {
        ZBytesIterator {
            reader: self.reader(),
            _t: PhantomData::<T>,
        }
    }

    /// Return an iterator on raw bytes slices contained in the [`ZBytes`].
    ///
    /// [`ZBytes`] may store data in non-contiguous regions of memory, this iterator
    /// then allows to access raw data directly without any attempt of deserializing it.
    /// Please note that no guarantee is provided on the internal memory layout of [`ZBytes`].
    /// The only provided guarantee is on the bytes order that is preserved.
    ///
    /// Please note that [`ZBytes::iter`] will perform deserialization while iterating while [`ZBytes::slices`] will not.
    ///
    /// ```rust
    /// use std::io::Write;
    /// use zenoh::bytes::ZBytes;
    ///
    /// let buf1: Vec<u8> = vec![1, 2, 3];
    /// let buf2: Vec<u8> = vec![4, 5, 6, 7, 8];
    /// let mut zbs = ZBytes::new();
    /// let mut writer = zbs.writer();
    /// writer.write(&buf1);
    /// writer.write(&buf2);
    ///
    /// // Access the raw content
    /// for slice in zbs.slices() {
    ///     println!("{:02x?}", slice);
    /// }
    ///
    /// // Concatenate input in a single vector
    /// let buf: Vec<u8> = buf1.into_iter().chain(buf2.into_iter()).collect();
    /// // Concatenate raw bytes in a single vector
    /// let out: Vec<u8> = zbs.slices().fold(Vec::new(), |mut b, x| { b.extend_from_slice(x); b });
    /// // The previous line is the equivalent of
    /// // let out: Vec<u8> = zbs.into();
    /// assert_eq!(buf, out);
    /// ```
    ///
    /// The example below shows how the [`ZBytesWriter::append`] simply appends the slices of one [`ZBytes`]
    /// to another and how those slices can be iterated over to access the raw data.
    /// ```rust
    /// use std::io::Write;
    /// use zenoh::bytes::ZBytes;
    ///
    /// let buf1: Vec<u8> = vec![1, 2, 3];
    /// let buf2: Vec<u8> = vec![4, 5, 6, 7, 8];
    ///
    /// let mut zbs = ZBytes::new();
    /// let mut writer = zbs.writer();
    /// writer.append(ZBytes::from(buf1.clone()));
    /// writer.append(ZBytes::from(buf2.clone()));
    ///
    /// let mut iter = zbs.slices();
    /// assert_eq!(buf1.as_slice(), iter.next().unwrap());
    /// assert_eq!(buf2.as_slice(), iter.next().unwrap());
    /// ```
    pub fn slices(&self) -> ZBytesSliceIterator<'_> {
        ZBytesSliceIterator(self.0.slices())
    }

    /// Serialize an object of type `T` as a [`ZBytes`] using the [`ZSerde`].
    ///
    /// ```rust
    /// use zenoh::bytes::ZBytes;
    ///
    /// let start = String::from("abc");
    /// let bytes = ZBytes::serialize(start.clone());
    /// let end: String = bytes.try_deserialize().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn serialize<'a, T: Serialize<'a, Error = Infallible>>(t: T) -> Self {
        Self::try_serialize(t).unwrap()
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
    /// let end: Value = bytes.try_deserialize().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn try_serialize<'a, T: Serialize<'a>>(t: T) -> Result<Self, T::Error> {
        if T::FIXED_SIZE.is_some() {
            let mut zbytes = ZBytes::new();
            t.write(&mut zbytes.writer())?;
            Ok(zbytes)
        } else {
            Ok(match t.serialize()? {
                Serialized::Slice(s) => s.into(),
                Serialized::ZBytes(zbytes) => zbytes,
            })
        }
    }

    /// Deserialize an object of type `T` using [`ZSerde`].
    ///
    /// See [`ZBytes::serialize`] and [`ZBytes::try_serialize`] for the examples.
    ///
    /// See [`ZBytes::deserialize`] for infallible conversion, e.g. to get raw bytes.
    pub fn try_deserialize<'a, T: Deserialize<'a>>(&'a self) -> Result<T, ZDeserializeError> {
        if T::FIXED_SIZE.is_some() {
            T::read(&mut self.reader())
        } else {
            T::deserialize(self)
        }
    }

    /// Infallibly deserialize an object of type `T` using [`ZSerde`].
    ///
    /// To directly access raw data as contiguous slice it is preferred to convert `ZBytes` into a [`std::borrow::Cow<[u8]>`](`std::borrow::Cow`).
    /// If [`ZBytes`] contains all the data in a single memory location, then it is guaranteed to be zero-copy. This is the common case for small messages.
    /// If [`ZBytes`] contains data scattered in different memory regions, this operation will do an allocation and a copy. This is the common case for large messages.
    ///
    /// ```rust
    /// use std::borrow::Cow;
    /// use zenoh::bytes::ZBytes;
    ///
    /// let buf: Vec<u8> = vec![0, 1, 2, 3];
    /// let bytes = ZBytes::from(buf.clone());
    /// let deser: Cow<[u8]> = bytes.into();
    /// assert_eq!(buf.as_slice(), deser.as_ref());
    /// ```
    ///
    /// An alternative is to convert `ZBytes` into a [`std::vec::Vec<u8>`].
    /// Converting to [`std::vec::Vec<u8>`] will always allocate and make a copy.
    ///
    /// ```rust
    /// use std::borrow::Cow;
    /// use zenoh::bytes::ZBytes;
    ///
    /// let buf: Vec<u8> = vec![0, 1, 2, 3];
    /// let bytes = ZBytes::from(buf.clone());
    /// let deser: Vec<u8> = bytes.deserialize();
    /// assert_eq!(buf.as_slice(), deser.as_slice());
    /// ```
    ///
    /// If you want to be sure that no copy is performed at all, then you should use [`ZBytes::slices`].
    /// Please note that in this case data may not be contiguous in memory and it is the responsibility of the user to properly parse the raw slices.
    pub fn deserialize<'a, T: Deserialize<'a>>(&'a self) -> T {
        self.try_deserialize().unwrap()
    }

    #[cfg(all(feature = "shared-memory", feature = "unstable"))]
    pub fn asz_shm(&self) -> Result<&zshm, ZDeserializeError> {
        // A ZShm is expected to have only one slice
        let mut zslices = self.0.zslices();
        if let Some(zs) = zslices.next() {
            if let Some(shmb) = zs.downcast_ref::<ShmBufInner>() {
                return Ok(shmb.into());
            }
        }
        Err(ZDeserializeError)
    }

    #[cfg(all(feature = "shared-memory", feature = "unstable"))]
    pub fn as_mut_zshm(&mut self) -> Result<&mut zshm, ZDeserializeError> {
        // A ZSliceShmBorrowMut is expected to have only one slice
        let mut zslices = self.0.zslices_mut();
        if let Some(zs) = zslices.next() {
            // SAFETY: ShmBufInner cannot change the size of the slice
            if let Some(shmb) = unsafe { zs.downcast_mut::<ShmBufInner>() } {
                return Ok(shmb.into());
            }
        }
        Err(ZDeserializeError)
    }

    #[cfg(all(feature = "shared-memory", feature = "unstable"))]
    pub fn as_zshm_mut(&mut self) -> Result<&mut zshmmut, ZDeserializeError> {
        // A ZSliceShmBorrowMut is expected to have only one slice
        let mut zslices = self.0.zslices_mut();
        if let Some(zs) = zslices.next() {
            // SAFETY: ShmBufInner cannot change the size of the slice
            if let Some(shmb) = unsafe { zs.downcast_mut::<ShmBufInner>() } {
                return shmb.try_into().or(Err(ZDeserializeError));
            }
        }
        Err(ZDeserializeError)
    }
}

/// A reader that implements [`std::io::Read`] trait to read from a [`ZBytes`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesReader<'a>(ZBufReader<'a>);

impl ZBytesReader<'_> {
    /// Returns the number of bytes that can still be read
    pub fn remaining(&self) -> usize {
        self.0.remaining()
    }

    /// Returns true if no more bytes can be read
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn deserialize<'a, T: Deserialize<'a>>(&mut self) -> T {
        self.try_deserialize().unwrap()
    }

    /// Deserialize an object of type `T` from a [`ZBytesReader`] using the [`ZSerde`].
    /// See [`ZBytesWriter::serialize`] for an example.
    pub fn try_deserialize<'a, T: Deserialize<'a>>(&mut self) -> Result<T, ZDeserializeError> {
        if T::FIXED_SIZE.is_some() {
            T::read(self).map_err(Into::into)
        } else {
            let codec = Zenoh080::new();
            let buf: ZBuf = codec.read(&mut self.0).or(Err(ZDeserializeError))?;
            T::try_from_zbytes(buf.into())
        }
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
    fn write_serialized(&mut self, serialized: Serialized) {
        let codec = Zenoh080::new();
        // Result can be ignored because writing on `ZBufWriter` only fails if there
        // is no bytes written.
        let _ = match serialized {
            Serialized::Slice(s) => codec.write(&mut self.0, s),
            Serialized::ZBytes(zbytes) => codec.write(&mut self.0, &zbytes.0),
        };
    }

    /// Serialize a type `T` on the [`ZBytes`]. For simmetricity, every serialization
    /// operation preserves type boundaries by preprending the length of the serialized data.
    /// This allows calling [`ZBytesReader::deserialize`] in the same order to retrieve the original type.
    ///
    /// Example:
    /// ```
    /// use zenoh::bytes::ZBytes;
    ///
    /// // serialization
    /// let mut bytes = ZBytes::new();
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
    pub fn serialize<'a, T: Serialize<'a, Error = Infallible>>(&mut self, t: T) {
        self.try_serialize(t).unwrap()
    }

    /// Try to serialize a type `T` on the [`ZBytes`]. Serialization works
    /// in the same way as [`ZBytesWriter::serialize`].
    pub fn try_serialize<'a, T: Serialize<'a>>(&mut self, t: T) -> Result<(), T::Error> {
        if T::FIXED_SIZE.is_some() {
            t.write(self)
        } else {
            self.write_serialized(t.serialize()?);
            Ok(())
        }
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
    /// use zenoh::bytes::ZBytes;
    ///
    /// let one = ZBytes::from(vec![0, 1]);
    /// let two = ZBytes::from(vec![2, 3, 4, 5]);
    /// let three = ZBytes::from(vec![6, 7]);
    ///
    /// let mut bytes = ZBytes::new();
    /// let mut writer = bytes.writer();
    /// // Append data without copying by passing ownership
    /// writer.append(one);
    /// writer.append(two);
    /// writer.append(three);
    ///
    /// // deserialization
    /// let mut out: Vec<u8> = bytes.into();
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

    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional)
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

/// An iterator that implements [`std::iter::Iterator`] trait to iterate on [`&[u8]`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesSliceIterator<'a>(ZBytesSliceIteratorInner<'a>);

// Typedef to make clippy happy about complex type. Encapsulate inner `ZBufSliceOperator`.
type ZBytesSliceIteratorInner<'a> =
    std::iter::Map<core::slice::Iter<'a, ZSlice>, fn(&'a ZSlice) -> &'a [u8]>;

impl<'a> Iterator for ZBytesSliceIterator<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// An iterator that implements [`std::iter::Iterator`] trait to iterate on values `T` in a [`ZBytes`].
/// Note that [`ZBytes`] contains a serialized version of `T` and iterating over a [`ZBytes`] performs lazy deserialization.
#[repr(transparent)]
#[derive(Debug)]
pub struct ZBytesIterator<'a, T> {
    reader: ZBytesReader<'a>,
    _t: PhantomData<T>,
}

impl<'a, T: Deserialize<'a>> Iterator for ZBytesIterator<'_, T> {
    type Item = Result<T, ZDeserializeError>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(if T::FIXED_SIZE.is_some() {
            if self.reader.is_empty() {
                return None;
            }
            T::read(&mut self.reader).map_err(Into::into)
        } else {
            let codec = Zenoh080::new();
            let buf: ZBuf = codec.read(&mut self.reader.0).ok()?;
            T::try_from_zbytes(buf.into())
        })
    }
}

impl<'a, A: Serialize<'a, Error = Infallible>> FromIterator<A> for ZBytes {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut bytes = ZBytes::new();
        let mut writer = bytes.writer();
        if let Some(size) = A::FIXED_SIZE {
            writer.reserve(size * iter.size_hint().0);
            for t in iter {
                t.write(&mut writer).unwrap();
            }
        } else {
            for t in iter {
                writer.write_serialized(t.serialize().unwrap());
            }
        }
        bytes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ZDeserializeError;

impl std::fmt::Display for ZDeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Deserialize error")
    }
}

impl std::error::Error for ZDeserializeError {}
impl From<DidntRead> for ZDeserializeError {
    fn from(_value: DidntRead) -> Self {
        Self
    }
}

// ZBytes
impl SerializedSize for ZBytes {}
impl<'a> Serialize<'a> for ZBytes {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.into())
    }
}
impl<'a> Serialize<'a> for &ZBytes {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.clone().serialize()
    }
}
impl Deserialize<'_> for ZBytes {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.clone())
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes)
    }
}

// ZBuf
impl SerializedSize for ZBuf {}
impl<'a> Serialize<'a> for ZBuf {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(ZBytes::from(self).into())
    }
}
impl<'a> Serialize<'a> for &ZBuf {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.clone().serialize()
    }
}
impl Deserialize<'_> for ZBuf {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.clone().0)
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.0)
    }
}
impl From<ZBuf> for ZBytes {
    fn from(value: ZBuf) -> Self {
        Self(value)
    }
}
impl From<ZBytes> for ZBuf {
    fn from(value: ZBytes) -> Self {
        value.0
    }
}

// ZSlice
impl SerializedSize for ZSlice {}
impl<'a> Serialize<'a> for ZSlice {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(ZBytes::from(self).into())
    }
}
impl<'a> Serialize<'a> for &ZSlice {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.clone().serialize()
    }
}
impl Deserialize<'_> for ZSlice {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.0.to_zslice())
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}
impl From<ZSlice> for ZBytes {
    fn from(value: ZSlice) -> Self {
        ZBytes(value.into())
    }
}

// [u8; N]
impl<const N: usize> SerializedSize for [u8; N] {
    const FIXED_SIZE: Option<usize> = Some(N);
}
impl<'a, const N: usize> Serialize<'a> for [u8; N] {
    type Error = Infallible;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        std::io::Write::write(writer, &self).unwrap();
        Ok(())
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        unimplemented!()
    }
}
impl<const N: usize> Deserialize<'_> for [u8; N] {
    fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        let mut buf = [0; N];
        std::io::Read::read_exact(reader, &mut buf).or(Err(ZDeserializeError))?;
        Ok(buf)
    }
    fn deserialize(_zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn try_from_zbytes(_zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
}
impl<const N: usize> From<[u8; N]> for ZBytes {
    fn from(value: [u8; N]) -> Self {
        ZBytes(value.into())
    }
}

// Vec<u8>
impl SerializedSize for Vec<u8> {}
impl<'a> Serialize<'a> for Vec<u8> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(ZBytes::from(self).into())
    }
}
impl<'a> Serialize<'a> for &'a Vec<u8> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.as_slice().into())
    }
}
impl Deserialize<'_> for Vec<u8> {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.0.contiguous().into())
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}
impl From<Vec<u8>> for ZBytes {
    fn from(value: Vec<u8>) -> Self {
        ZBytes(value.into())
    }
}

// &[u8]
impl SerializedSize for &[u8] {}
impl<'a> Serialize<'a> for &'a [u8] {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.into())
    }
}
impl From<&[u8]> for ZBytes {
    fn from(value: &[u8]) -> Self {
        ZBytes::serialize(value)
    }
}

// Cow<[u8]>
impl SerializedSize for Cow<'_, [u8]> {}
impl<'a> Serialize<'a> for Cow<'a, [u8]> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(match self {
            Cow::Borrowed(s) => Serialized::Slice(s),
            Cow::Owned(vec) => Serialized::ZBytes(vec.into()),
        })
    }
}
impl<'a> Serialize<'a> for &'a Cow<'a, [u8]> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.as_ref().into())
    }
}
impl<'a> Deserialize<'a> for Cow<'a, [u8]> {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'a ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(zbytes.0.contiguous())
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Vec::try_from_zbytes(zbytes).map(Cow::Owned)
    }
}
impl From<Cow<'_, [u8]>> for ZBytes {
    fn from(value: Cow<'_, [u8]>) -> Self {
        ZBytes::serialize(value)
    }
}

// String
impl SerializedSize for String {}
impl<'a> Serialize<'a> for String {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.into_bytes().serialize()
    }
}
impl<'a> Serialize<'a> for &'a String {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.as_bytes().serialize()
    }
}
impl Deserialize<'_> for String {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        String::from_utf8(<Vec<u8>>::deserialize(zbytes)?).or(Err(ZDeserializeError))
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}
// &[u8]
impl SerializedSize for &str {}
impl<'a> Serialize<'a> for &'a str {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.as_bytes().into())
    }
}

// Cow<[u8]>
impl SerializedSize for Cow<'_, str> {}
impl<'a> Serialize<'a> for Cow<'a, str> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(match self {
            Cow::Borrowed(s) => Serialized::Slice(s.as_bytes()),
            Cow::Owned(vec) => Serialized::ZBytes(vec.into_bytes().into()),
        })
    }
}
impl<'a> Serialize<'a> for &'a Cow<'a, str> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.as_bytes().into())
    }
}
impl<'a> Deserialize<'a> for Cow<'a, str> {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'a ZBytes) -> Result<Self, ZDeserializeError> {
        Ok(match <Cow<'a, [u8]>>::deserialize(zbytes)? {
            Cow::Borrowed(s) => std::str::from_utf8(s).or(Err(ZDeserializeError))?.into(),
            Cow::Owned(vec) => String::from_utf8(vec).or(Err(ZDeserializeError))?.into(),
        })
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        String::try_from_zbytes(zbytes).map(Cow::Owned)
    }
}

// numeric
macro_rules! impl_num {
    ($($ty:ty),* $(,)?) => {$(
        impl SerializedSize for $ty {
            const FIXED_SIZE: Option<usize> = Some(size_of::<$ty>());
        }
        impl<'a> Serialize<'a> for $ty {
            type Error = Infallible;
            fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                self.to_le_bytes().write(writer)
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                unimplemented!()
            }
        }
        impl<'a> Serialize<'a> for &$ty {
            type Error = Infallible;
            fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                (*self).to_le_bytes().write(writer)
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                unimplemented!()
            }
        }
        impl Deserialize<'_> for $ty {
            fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
                <[u8; {size_of::<$ty>()}]>::read(reader).map(<$ty>::from_le_bytes)
            }
            fn deserialize(_zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
                unimplemented!()
            }
            fn try_from_zbytes(_zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
                unimplemented!()
            }
        }
    )*};
}
impl_num!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

impl SerializedSize for bool {
    const FIXED_SIZE: Option<usize> = Some(size_of::<bool>());
}
impl<'a> Serialize<'a> for bool {
    type Error = Infallible;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        (self as u8).write(writer)
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        unimplemented!()
    }
}
impl<'a> Serialize<'a> for &bool {
    type Error = Infallible;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        (*self as u8).write(writer)
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        unimplemented!()
    }
}
impl Deserialize<'_> for bool {
    fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        match u8::read(reader)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ZDeserializeError),
        }
    }
    fn deserialize(_zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn try_from_zbytes(_zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
}

// char
impl SerializedSize for char {
    const FIXED_SIZE: Option<usize> = Some(size_of::<char>());
}
impl<'a> Serialize<'a> for char {
    type Error = Infallible;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        u32::from(self).write(writer)
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        unimplemented!()
    }
}
impl<'a> Serialize<'a> for &char {
    type Error = Infallible;
    fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        u32::from(*self).write(writer)
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        unimplemented!()
    }
}
impl Deserialize<'_> for char {
    fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        u32::read(reader)?.try_into().or(Err(ZDeserializeError))
    }
    fn deserialize(_zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn try_from_zbytes(_zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
}

/* Zenoh advanced types serializer/deserializer */
// Parameters
impl SerializedSize for Parameters<'_> {}
impl<'a> Serialize<'a> for Parameters<'a> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        <Cow<'a, str>>::from(self).serialize()
    }
}
impl<'a> Serialize<'a> for &'a Parameters<'a> {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(self.as_str().as_bytes().into())
    }
}
impl<'a> Deserialize<'a> for Parameters<'a> {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'a ZBytes) -> Result<Self, ZDeserializeError> {
        <Cow<'a, str>>::deserialize(zbytes).map(Into::into)
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        String::try_from_zbytes(zbytes).map(Into::into)
    }
}

// Timestamp
impl SerializedSize for Timestamp {}
impl<'a> Serialize<'a> for Timestamp {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        (&self).serialize()
    }
}
impl<'a> Serialize<'a> for &Timestamp {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        let codec = Zenoh080::new();
        let mut buffer = ZBuf::empty();
        let mut writer = buffer.writer();
        // Result can be ignored because writing on `ZBufWriter` only fails if there
        // is no bytes written.
        let _ = codec.write(&mut writer, self);
        Ok(ZBytes::from(buffer).into())
    }
}
impl Deserialize<'_> for Timestamp {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        let codec = Zenoh080::new();
        let mut reader = zbytes.0.reader();
        codec.read(&mut reader).or(Err(ZDeserializeError))
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}

// Encoding
impl SerializedSize for Encoding {}
impl<'a> Serialize<'a> for Encoding {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        let encoding: EncodingProto = self.into();
        let codec = Zenoh080::new();
        let mut buffer = ZBuf::empty();
        let mut writer = buffer.writer();
        // Result can be ignored because writing on `ZBufWriter` only fails if there
        // is no bytes written.
        let _ = codec.write(&mut writer, &encoding);
        Ok(ZBytes::from(buffer).into())
    }
}
impl<'a> Serialize<'a> for &Encoding {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.clone().serialize()
    }
}
impl Deserialize<'_> for Encoding {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        let codec = Zenoh080::new();
        let mut reader = zbytes.0.reader();
        let encoding: EncodingProto = codec.read(&mut reader).or(Err(ZDeserializeError))?;
        Ok(encoding.into())
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}

// Value
impl SerializedSize for Value {}
impl<'a> Serialize<'a> for Value {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        (self.payload, self.encoding).serialize()
    }
}
impl<'a> Serialize<'a> for &Value {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        (&self.payload, &self.encoding).serialize()
    }
}
impl Deserialize<'_> for Value {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        let (payload, encoding) = <_>::deserialize(zbytes)?;
        Ok(Value { payload, encoding })
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}

// JSON/YAML
macro_rules! impl_serde {
    ($($package:ident),* $(,)?) => {$(
        impl SerializedSize for $package::Value {}
        impl<'a> Serialize<'a> for $package::Value {
            type Error = $package::Error;
            fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                unimplemented!()
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                (&self).serialize()
            }
        }
        impl<'a> Serialize<'a> for &$package::Value {
            type Error = $package::Error;
            fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                unimplemented!()
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                let mut zbytes = ZBytes::new();
                $package::to_writer(zbytes.writer(), self)?;
                Ok(zbytes.into())
            }
        }
        impl Deserialize<'_> for $package::Value {
            fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
                unimplemented!()
            }
            fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
                Ok($package::from_reader(zbytes.reader()).or(Err(ZDeserializeError))?)
            }
            fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
                Self::deserialize(&zbytes)
            }
        }
    )*};
}
impl_serde!(serde_json, serde_yaml);

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

impl SerializedSize for bytes::Bytes {}
impl<'a> Serialize<'a> for bytes::Bytes {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        Ok(ZBytes::from(self).into())
    }
}
impl<'a> Serialize<'a> for &bytes::Bytes {
    type Error = Infallible;
    fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
        unimplemented!()
    }
    fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
        self.clone().serialize()
    }
}
impl Deserialize<'_> for bytes::Bytes {
    fn read(_reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        unimplemented!()
    }
    fn deserialize(zbytes: &'_ ZBytes) -> Result<Self, ZDeserializeError> {
        // bytes::Bytes can be constructed only by passing ownership to the constructor.
        // Thereofore, here we are forced to allocate a vector and copy the whole ZBytes
        // content since bytes::Bytes does not support anything else than Box<u8> (and its
        // variants like Vec<u8> and String).
        Ok(bytes::Bytes::from(<Vec<u8>>::deserialize(zbytes)?))
    }
    fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
        Self::deserialize(&zbytes)
    }
}
impl From<bytes::Bytes> for ZBytes {
    fn from(value: bytes::Bytes) -> Self {
        ZBytes(BytesWrap(value).into())
    }
}

// Shared memory
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
mod shm {
    use zenoh_shm::api::buffer::{zshm::ZShm, zshmmut::ZShmMut};

    use super::*;

    impl SerializedSize for ZShm {}
    impl<'a> Serialize<'a> for ZShm {
        type Error = Infallible;
        fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
            unimplemented!()
        }
        fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
            Ok(ZBytes::from(self).into())
        }
    }
    impl<'a> Serialize<'a> for &ZShm {
        type Error = Infallible;
        fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
            unimplemented!()
        }
        fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
            self.clone().serialize()
        }
    }
    impl From<ZShm> for ZBytes {
        fn from(value: ZShm) -> Self {
            ZBytes(value.into())
        }
    }

    impl SerializedSize for ZShmMut {}
    impl<'a> Serialize<'a> for ZShmMut {
        type Error = Infallible;
        fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
            unimplemented!()
        }
        fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
            Ok(ZBytes::from(self).into())
        }
    }
    impl From<ZShmMut> for ZBytes {
        fn from(value: ZShmMut) -> Self {
            ZBytes(value.into())
        }
    }

    impl SerializedSize for &zshm {}
    impl<'a> Serialize<'a> for &zshm {
        type Error = Infallible;
        fn write(self, _writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
            unimplemented!()
        }
        fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
            Ok(ZBytes::from(self).into())
        }
    }
    impl From<&zshm> for ZBytes {
        fn from(value: &zshm) -> Self {
            value.to_owned().into()
        }
    }
}

macro_rules! impl_tuple {
    ($($ty:ident/$i:tt),* $(,)?) => {
        impl_tuple!(@;$($ty/$i),*);
    };
    (@$($ty:ident/$i:tt),*; $next:ident/$next_i:tt $(, $remain:ident/$remain_i:tt)*) => {
        impl_tuple!(@@$($ty/$i),*);
        impl_tuple!(@$($ty/$i,)* $next/$next_i; $($remain/$remain_i),*);
    };
    (@$($ty:ident/$i:tt),*;) => {
        impl_tuple!(@@$($ty/$i),*);
    };
    (@@$($ty:ident/$i:tt),* $(,)?) => {
        impl<$($ty: SerializedSize),*> SerializedSize for ($($ty,)*) {
            const FIXED_SIZE: Option<usize> = loop {
                $(if $ty::FIXED_SIZE.is_none() { break None; })*
                break Some(0 $(+ match $ty::FIXED_SIZE { Some(s) => s, None => unreachable!()})*)
            };
        }
        #[allow(unused)]
        impl<'a, $($ty: Serialize<'a, Error=Infallible>),*> Serialize<'a> for ($($ty,)*) {
            type Error = Infallible;
            fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                $(self.$i.write(writer)?;)*
                Ok(())
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                let mut zbytes = ZBytes::new();
                let mut writer = zbytes.writer();
                $(writer.serialize(self.$i);)*
                Ok(zbytes.into())
            }
        }
        #[allow(unused)]
        impl<'a, $($ty: SerializedSize),*> Serialize<'a> for &'a ($($ty,)*) where $(&'a $ty: Serialize<'a, Error=Infallible>),* {
            type Error = Infallible;
            fn write(self, writer: &mut ZBytesWriter) -> Result<(), Self::Error> {
                $(self.$i.write(writer)?;)*
                Ok(())
            }
            fn serialize(self) -> Result<Serialized<'a>, Self::Error> {
                let mut zbytes = ZBytes::new();
                let mut writer = zbytes.writer();
                $(writer.serialize(&self.$i);)*
                Ok(zbytes.into())
            }
        }
        #[allow(unused)]
        impl<'a, $($ty: Deserialize<'a>),*> Deserialize<'a> for ($($ty,)*) {
            fn read(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
                Ok(($($ty::read(reader)?,)*))
            }
            fn deserialize(zbytes: &'a ZBytes) -> Result<Self, ZDeserializeError> {
                let mut reader = zbytes.reader();
                Ok(($(reader.try_deserialize::<$ty>()?,)*))
            }
            fn try_from_zbytes(zbytes: ZBytes) -> Result<Self, ZDeserializeError> {
                let mut reader = zbytes.reader();
                Ok(($(reader.try_deserialize::<$ty>()?,)*))
            }
        }
    };
}
impl_tuple!(T0 / 0, T1 / 1, T2 / 2,);
// T3 / 3,
// T4 / 4,
// T5 / 5,
// T6 / 6,
// T7 / 7,
// T8 / 8,
// T9 / 9,
// T10 / 10,
// T11 / 11,
// T12 / 12,
// T13 / 13,
// T14 / 14,
// T15 / 15

// TODO do we really need this?
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
        #[cfg(feature = "shared-memory")]
        use crate::zenoh_core::Wait;

        const NUM: usize = 1_000;

        macro_rules! serialize_deserialize {
            ($t:ty, $in:expr) => {
                let i = $in;
                let t = i.clone();
                println!("Serialize:\t{:?}", t);
                let v = ZBytes::serialize(t);
                println!("Deserialize:\t{:?}", v);
                let o: $t = v.try_deserialize().unwrap();
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
            let mut bytes = ZBytes::new();
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
                .wait()
                .unwrap();
            // ...and an SHM provider
            let provider = ShmProviderBuilder::builder()
                .protocol_id::<POSIX_PROTOCOL_ID>()
                .backend(backend)
                .wait();

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
            let o = p.try_deserialize::<HashMap<usize, usize>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, Vec<u8>> = HashMap::new();
            hm.insert(0, vec![0u8; 8]);
            hm.insert(1, vec![1u8; 16]);
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.try_deserialize::<HashMap<usize, Vec<u8>>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, ZSlice> = HashMap::new();
            hm.insert(0, ZSlice::from(vec![0u8; 8]));
            hm.insert(1, ZSlice::from(vec![1u8; 16]));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.try_deserialize::<HashMap<usize, ZSlice>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<usize, ZBuf> = HashMap::new();
            hm.insert(0, ZBuf::from(vec![0u8; 8]));
            hm.insert(1, ZBuf::from(vec![1u8; 16]));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.try_deserialize::<HashMap<usize, ZBuf>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<String, String> = HashMap::new();
            hm.insert(String::from("0"), String::from("a"));
            hm.insert(String::from("1"), String::from("b"));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.try_deserialize::<HashMap<String, String>>().unwrap();
            assert_eq!(hm, o);

            let mut hm: HashMap<Cow<'static, str>, Cow<'static, str>> = HashMap::new();
            hm.insert(Cow::from("0"), Cow::from("a"));
            hm.insert(Cow::from("1"), Cow::from("b"));
            println!("Serialize:\t{:?}", hm);
            let p = ZBytes::from(hm.clone());
            println!("Deserialize:\t{:?}\n", p);
            let o = p.try_deserialize::<HashMap<Cow<str>, Cow<str>>>().unwrap();
            assert_eq!(hm, o);
        }
        hashmap();
    }
}
