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

//! Payload primitives.
use crate::buffers::ZBuf;
use std::{
    borrow::Cow, convert::Infallible, fmt::Debug, ops::Deref, string::FromUtf8Error, sync::Arc,
};
use zenoh_buffers::buffer::Buffer;
use zenoh_buffers::{
    buffer::SplitBuffer, reader::HasReader, writer::HasWriter, ZBufReader, ZSlice,
};
use zenoh_result::ZResult;
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

#[repr(transparent)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Payload(ZBuf);

impl Payload {
    /// Create an empty payload.
    pub const fn empty() -> Self {
        Self(ZBuf::empty())
    }

    /// Create a [`Payload`] from any type `T` that can implements [`Into<ZBuf>`].
    pub fn new<T>(t: T) -> Self
    where
        T: Into<ZBuf>,
    {
        Self(t.into())
    }

    /// Returns wether the payload is empty or not.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the length of the payload.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Get a [`PayloadReader`] implementing [`std::io::Read`] trait.
    pub fn reader(&self) -> PayloadReader<'_> {
        PayloadReader(self.0.reader())
    }
}

/// A reader that implements [`std::io::Read`] trait to read from a [`Payload`].
pub struct PayloadReader<'a>(ZBufReader<'a>);

impl std::io::Read for PayloadReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

/// Provide some facilities specific to the Rust API to encode/decode a [`Value`] with an `Serialize`.
impl Payload {
    /// Encode an object of type `T` as a [`Value`] using the [`ZSerde`].
    ///
    /// ```rust
    /// use zenoh::payload::Payload;
    ///
    /// let start = String::from("abc");
    /// let payload = Payload::serialize(start.clone());
    /// let end: String = payload.deserialize().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn serialize<T>(t: T) -> Self
    where
        ZSerde: Serialize<T, Output = Payload>,
    {
        ZSerde.serialize(t)
    }

    /// Decode an object of type `T` from a [`Value`] using the [`ZSerde`].
    /// See [encode](Value::encode) for an example.
    pub fn deserialize<'a, T>(&'a self) -> ZResult<T>
    where
        ZSerde: Deserialize<'a, T>,
        <ZSerde as Deserialize<'a, T>>::Error: Debug,
    {
        let t: T = ZSerde.deserialize(self).map_err(|e| zerror!("{:?}", e))?;
        Ok(t)
    }
}

/// Trait to encode a type `T` into a [`Value`].
pub trait Serialize<T> {
    type Output;

    /// The implementer should take care of serializing the type `T` and set the proper [`Encoding`].
    fn serialize(self, t: T) -> Self::Output;
}

pub trait Deserialize<'a, T> {
    type Error;

    /// The implementer should take care of deserializing the type `T` based on the [`Encoding`] information.
    fn deserialize(self, t: &'a Payload) -> Result<T, Self::Error>;
}

/// The default serializer for Zenoh payload. It supports primitives types, such as: vec<u8>, int, uint, float, string, bool.
/// It also supports common Rust serde values.
#[derive(Clone, Copy, Debug)]
pub struct ZSerde;

#[derive(Debug, Clone, Copy)]
pub struct ZDeserializeError;

// Bytes
impl Serialize<ZBuf> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: ZBuf) -> Self::Output {
        Payload::new(t)
    }
}

impl From<Payload> for ZBuf {
    fn from(value: Payload) -> Self {
        value.0
    }
}

impl Deserialize<'_, ZBuf> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &Payload) -> Result<ZBuf, Self::Error> {
        Ok(v.into())
    }
}

impl From<&Payload> for ZBuf {
    fn from(value: &Payload) -> Self {
        value.0.clone()
    }
}

impl Serialize<Vec<u8>> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: Vec<u8>) -> Self::Output {
        Payload::new(t)
    }
}

impl Serialize<&[u8]> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: &[u8]) -> Self::Output {
        Payload::new(t.to_vec())
    }
}

impl Deserialize<'_, Vec<u8>> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &Payload) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::from(v))
    }
}

impl From<&Payload> for Vec<u8> {
    fn from(value: &Payload) -> Self {
        Cow::from(value).to_vec()
    }
}

impl<'a> Serialize<Cow<'a, [u8]>> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: Cow<'a, [u8]>) -> Self::Output {
        Payload::new(t.to_vec())
    }
}

impl<'a> Deserialize<'a, Cow<'a, [u8]>> for ZSerde {
    type Error = Infallible;

    fn deserialize(self, v: &'a Payload) -> Result<Cow<'a, [u8]>, Self::Error> {
        Ok(Cow::from(v))
    }
}

impl<'a> From<&'a Payload> for Cow<'a, [u8]> {
    fn from(value: &'a Payload) -> Self {
        value.0.contiguous()
    }
}

// String
impl Serialize<String> for ZSerde {
    type Output = Payload;

    fn serialize(self, s: String) -> Self::Output {
        Payload::new(s.into_bytes())
    }
}

impl Serialize<&str> for ZSerde {
    type Output = Payload;

    fn serialize(self, s: &str) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl Deserialize<'_, String> for ZSerde {
    type Error = FromUtf8Error;

    fn deserialize(self, v: &Payload) -> Result<String, Self::Error> {
        String::from_utf8(Vec::from(v))
    }
}

impl TryFrom<&Payload> for String {
    type Error = FromUtf8Error;

    fn try_from(value: &Payload) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

impl TryFrom<Payload> for String {
    type Error = FromUtf8Error;

    fn try_from(value: Payload) -> Result<Self, Self::Error> {
        ZSerde.deserialize(&value)
    }
}

impl<'a> Serialize<Cow<'a, str>> for ZSerde {
    type Output = Payload;

    fn serialize(self, s: Cow<'a, str>) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl<'a> Deserialize<'a, Cow<'a, str>> for ZSerde {
    type Error = FromUtf8Error;

    fn deserialize(self, v: &Payload) -> Result<Cow<'a, str>, Self::Error> {
        let v: String = Self.deserialize(v)?;
        Ok(Cow::Owned(v))
    }
}

impl<'a> TryFrom<&'a Payload> for Cow<'a, str> {
    type Error = FromUtf8Error;

    fn try_from(value: &'a Payload) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// - Integers impl
macro_rules! impl_int {
    ($t:ty, $encoding:expr) => {
        impl Serialize<$t> for ZSerde {
            type Output = Payload;

            fn serialize(self, t: $t) -> Self::Output {
                let bs = t.to_le_bytes();
                let end = 1 + bs.iter().rposition(|b| *b != 0).unwrap_or(bs.len() - 1);
                // SAFETY:
                // - 0 is a valid start index because bs is guaranteed to always have a length greater or equal than 1
                // - end is a valid end index because is bounded between 0 and bs.len()
                Payload::new(unsafe { ZSlice::new_unchecked(Arc::new(bs), 0, end) })
            }
        }

        impl Serialize<&$t> for ZSerde {
            type Output = Payload;

            fn serialize(self, t: &$t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl Serialize<&mut $t> for ZSerde {
            type Output = Payload;

            fn serialize(self, t: &mut $t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl<'a> Deserialize<'a, $t> for ZSerde {
            type Error = ZDeserializeError;

            fn deserialize(self, v: &Payload) -> Result<$t, Self::Error> {
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

        impl TryFrom<&Payload> for $t {
            type Error = ZDeserializeError;

            fn try_from(value: &Payload) -> Result<Self, Self::Error> {
                ZSerde.deserialize(value)
            }
        }
    };
}

// Zenoh unsigned integers
impl_int!(u8, ZSerde::ZENOH_UINT);
impl_int!(u16, ZSerde::ZENOH_UINT);
impl_int!(u32, ZSerde::ZENOH_UINT);
impl_int!(u64, ZSerde::ZENOH_UINT);
impl_int!(usize, ZSerde::ZENOH_UINT);

// Zenoh signed integers
impl_int!(i8, ZSerde::ZENOH_INT);
impl_int!(i16, ZSerde::ZENOH_INT);
impl_int!(i32, ZSerde::ZENOH_INT);
impl_int!(i64, ZSerde::ZENOH_INT);
impl_int!(isize, ZSerde::ZENOH_INT);

// Zenoh floats
impl_int!(f32, ZSerde::ZENOH_FLOAT);
impl_int!(f64, ZSerde::ZENOH_FLOAT);

// Zenoh bool
impl Serialize<bool> for ZSerde {
    type Output = ZBuf;

    fn serialize(self, t: bool) -> Self::Output {
        // SAFETY: casting a bool into an integer is well-defined behaviour.
        //      0 is false, 1 is true: https://doc.rust-lang.org/std/primitive.bool.html
        ZBuf::from((t as u8).to_le_bytes())
    }
}

impl Deserialize<'_, bool> for ZSerde {
    type Error = ZDeserializeError;

    fn deserialize(self, v: &Payload) -> Result<bool, Self::Error> {
        let p = v.deserialize::<u8>().map_err(|_| ZDeserializeError)?;
        match p {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ZDeserializeError),
        }
    }
}

impl TryFrom<&Payload> for bool {
    type Error = ZDeserializeError;

    fn try_from(value: &Payload) -> Result<Self, Self::Error> {
        ZSerde.deserialize(value)
    }
}

// - Zenoh advanced types encoders/decoders
// JSON
impl Serialize<&serde_json::Value> for ZSerde {
    type Output = Result<Payload, serde_json::Error>;

    fn serialize(self, t: &serde_json::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_json::to_writer(payload.0.writer(), t)?;
        Ok(payload)
    }
}

impl Serialize<serde_json::Value> for ZSerde {
    type Output = Result<Payload, serde_json::Error>;

    fn serialize(self, t: serde_json::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<'_, serde_json::Value> for ZSerde {
    type Error = serde_json::Error;

    fn deserialize(self, v: &Payload) -> Result<serde_json::Value, Self::Error> {
        serde_json::from_reader(v.reader())
    }
}

impl TryFrom<serde_json::Value> for Payload {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

// Yaml
impl Serialize<&serde_yaml::Value> for ZSerde {
    type Output = Result<Payload, serde_yaml::Error>;

    fn serialize(self, t: &serde_yaml::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_yaml::to_writer(payload.0.writer(), t)?;
        Ok(payload)
    }
}

impl Serialize<serde_yaml::Value> for ZSerde {
    type Output = Result<Payload, serde_yaml::Error>;

    fn serialize(self, t: serde_yaml::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<'_, serde_yaml::Value> for ZSerde {
    type Error = serde_yaml::Error;

    fn deserialize(self, v: &Payload) -> Result<serde_yaml::Value, Self::Error> {
        serde_yaml::from_reader(v.reader())
    }
}

impl TryFrom<serde_yaml::Value> for Payload {
    type Error = serde_yaml::Error;

    fn try_from(value: serde_yaml::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

// CBOR
impl Serialize<&serde_cbor::Value> for ZSerde {
    type Output = Result<Payload, serde_cbor::Error>;

    fn serialize(self, t: &serde_cbor::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_cbor::to_writer(payload.0.writer(), t)?;
        Ok(payload)
    }
}

impl Serialize<serde_cbor::Value> for ZSerde {
    type Output = Result<Payload, serde_cbor::Error>;

    fn serialize(self, t: serde_cbor::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<'_, serde_cbor::Value> for ZSerde {
    type Error = serde_cbor::Error;

    fn deserialize(self, v: &Payload) -> Result<serde_cbor::Value, Self::Error> {
        serde_cbor::from_reader(v.reader())
    }
}

impl TryFrom<serde_cbor::Value> for Payload {
    type Error = serde_cbor::Error;

    fn try_from(value: serde_cbor::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

// Pickle
impl Serialize<&serde_pickle::Value> for ZSerde {
    type Output = Result<Payload, serde_pickle::Error>;

    fn serialize(self, t: &serde_pickle::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_pickle::value_to_writer(
            &mut payload.0.writer(),
            t,
            serde_pickle::SerOptions::default(),
        )?;
        Ok(payload)
    }
}

impl Serialize<serde_pickle::Value> for ZSerde {
    type Output = Result<Payload, serde_pickle::Error>;

    fn serialize(self, t: serde_pickle::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<'_, serde_pickle::Value> for ZSerde {
    type Error = serde_pickle::Error;

    fn deserialize(self, v: &Payload) -> Result<serde_pickle::Value, Self::Error> {
        serde_pickle::value_from_reader(v.reader(), serde_pickle::DeOptions::default())
    }
}

impl TryFrom<serde_pickle::Value> for Payload {
    type Error = serde_pickle::Error;

    fn try_from(value: serde_pickle::Value) -> Result<Self, Self::Error> {
        ZSerde.serialize(value)
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Serialize<Arc<SharedMemoryBuf>> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: Arc<SharedMemoryBuf>) -> Self::Output {
        Payload::new(t)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<Box<SharedMemoryBuf>> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: Box<SharedMemoryBuf>) -> Self::Output {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self.serialize(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<SharedMemoryBuf> for ZSerde {
    type Output = Payload;

    fn serialize(self, t: SharedMemoryBuf) -> Self::Output {
        Payload::new(t)
    }
}

impl<T> From<T> for Payload
where
    ZSerde: Serialize<T, Output = Payload>,
{
    fn from(t: T) -> Self {
        ZSerde.serialize(t)
    }
}

// For convenience to always convert a Value the examples
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

impl From<&Payload> for StringOrBase64 {
    fn from(v: &Payload) -> Self {
        use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
        match v.deserialize::<Cow<str>>() {
            Ok(s) => StringOrBase64::String(s.into_owned()),
            Err(_) => {
                let cow: Cow<'_, [u8]> = Cow::from(v);
                StringOrBase64::Base64(b64_std_engine.encode(cow))
            }
        }
    }
}

mod tests {
    #[test]
    fn serializer() {
        use super::Payload;
        use rand::Rng;
        use zenoh_buffers::ZBuf;

        const NUM: usize = 1_000;

        macro_rules! serialize_deserialize {
            ($t:ty, $in:expr) => {
                let i = $in;
                let t = i.clone();
                let v = Payload::serialize(t);
                let o: $t = v.deserialize().unwrap();
                assert_eq!(i, o)
            };
        }

        let mut rng = rand::thread_rng();

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

        serialize_deserialize!(f32, f32::MIN);
        serialize_deserialize!(f64, f64::MIN);

        serialize_deserialize!(f32, f32::MAX);
        serialize_deserialize!(f64, f64::MAX);

        for _ in 0..NUM {
            serialize_deserialize!(f32, rng.gen::<f32>());
            serialize_deserialize!(f64, rng.gen::<f64>());
        }

        serialize_deserialize!(String, "");
        serialize_deserialize!(String, String::from("abcdefghijklmnopqrstuvwxyz"));

        serialize_deserialize!(Vec<u8>, vec![0u8; 0]);
        serialize_deserialize!(Vec<u8>, vec![0u8; 64]);

        serialize_deserialize!(ZBuf, ZBuf::from(vec![0u8; 0]));
        serialize_deserialize!(ZBuf, ZBuf::from(vec![0u8; 64]));
    }
}
