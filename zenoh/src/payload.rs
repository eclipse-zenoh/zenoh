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
    borrow::Cow,
    convert::Infallible,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use zenoh_buffers::{buffer::SplitBuffer, reader::HasReader, writer::HasWriter, ZSlice};
use zenoh_result::{ZError, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Payload(ZBuf);

impl Payload {
    pub const fn empty() -> Self {
        Self(ZBuf::empty())
    }

    pub fn new<T>(t: T) -> Self
    where
        T: Into<ZBuf>,
    {
        Self(t.into())
    }
}

impl Deref for Payload {
    type Target = ZBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Payload {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Trait to encode a type `T` into a [`Value`].
pub trait Serialize<T> {
    type Output;

    /// The implementer should take care of serializing the type `T` and set the proper [`Encoding`].
    fn serialize(self, t: T) -> Self::Output;
}

pub trait Deserialize<T> {
    type Error;

    /// The implementer should take care of deserializing the type `T` based on the [`Encoding`] information.
    fn deserialize(self, t: &Payload) -> Result<T, Self::Error>;
}

// Octect stream
impl Serialize<ZBuf> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: ZBuf) -> Self::Output {
        Payload::new(t)
    }
}

impl Deserialize<ZBuf> for DefaultSerializer {
    type Error = Infallible;

    fn deserialize(self, v: &Payload) -> Result<ZBuf, Self::Error> {
        Ok(v.into())
    }
}

impl Serialize<Vec<u8>> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: Vec<u8>) -> Self::Output {
        Payload::new(t)
    }
}

impl Serialize<&[u8]> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: &[u8]) -> Self::Output {
        Payload::new(t.to_vec())
    }
}

impl Deserialize<Vec<u8>> for DefaultSerializer {
    type Error = Infallible;

    fn deserialize(self, v: &Payload) -> Result<Vec<u8>, Self::Error> {
        let v: ZBuf = v.into();
        Ok(v.contiguous().to_vec())
    }
}

impl<'a> Serialize<Cow<'a, [u8]>> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: Cow<'a, [u8]>) -> Self::Output {
        Payload::new(t.to_vec())
    }
}

impl<'a> Deserialize<Cow<'a, [u8]>> for DefaultSerializer {
    type Error = Infallible;

    fn deserialize(self, v: &Payload) -> Result<Cow<'a, [u8]>, Self::Error> {
        let v: Vec<u8> = Self.deserialize(v)?;
        Ok(Cow::Owned(v))
    }
}

// Text plain
impl Serialize<String> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, s: String) -> Self::Output {
        Payload::new(s.into_bytes())
    }
}

impl Serialize<&str> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, s: &str) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl Deserialize<String> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<String, Self::Error> {
        String::from_utf8(v.contiguous().to_vec()).map_err(|e| zerror!("{}", e.utf8_error()))
    }
}

impl<'a> Serialize<Cow<'a, str>> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, s: Cow<'a, str>) -> Self::Output {
        Self.serialize(s.to_string())
    }
}

impl<'a> Deserialize<Cow<'a, str>> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<Cow<'a, str>, Self::Error> {
        let v: String = Self.deserialize(v)?;
        Ok(Cow::Owned(v))
    }
}

// - Integers impl
macro_rules! impl_int {
    ($t:ty, $encoding:expr) => {
        impl Serialize<$t> for DefaultSerializer {
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

        impl Serialize<&$t> for DefaultSerializer {
            type Output = Payload;

            fn serialize(self, t: &$t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl Serialize<&mut $t> for DefaultSerializer {
            type Output = Payload;

            fn serialize(self, t: &mut $t) -> Self::Output {
                Self.serialize(*t)
            }
        }

        impl Deserialize<$t> for DefaultSerializer {
            type Error = ZError;

            fn deserialize(self, v: &Payload) -> Result<$t, Self::Error> {
                let p = v.contiguous();
                let mut bs = (0 as $t).to_le_bytes();
                if p.len() > bs.len() {
                    bail!(
                        "Decode error: {} invalid length ({} > {})",
                        std::any::type_name::<$t>(),
                        p.len(),
                        bs.len()
                    );
                }
                bs[..p.len()].copy_from_slice(&p);
                let t = <$t>::from_le_bytes(bs);
                Ok(t)
            }
        }
    };
}

// Zenoh unsigned integers
impl_int!(u8, DefaultSerializer::ZENOH_UINT);
impl_int!(u16, DefaultSerializer::ZENOH_UINT);
impl_int!(u32, DefaultSerializer::ZENOH_UINT);
impl_int!(u64, DefaultSerializer::ZENOH_UINT);
impl_int!(usize, DefaultSerializer::ZENOH_UINT);

// Zenoh signed integers
impl_int!(i8, DefaultSerializer::ZENOH_INT);
impl_int!(i16, DefaultSerializer::ZENOH_INT);
impl_int!(i32, DefaultSerializer::ZENOH_INT);
impl_int!(i64, DefaultSerializer::ZENOH_INT);
impl_int!(isize, DefaultSerializer::ZENOH_INT);

// Zenoh floats
impl_int!(f32, DefaultSerializer::ZENOH_FLOAT);
impl_int!(f64, DefaultSerializer::ZENOH_FLOAT);

// Zenoh bool
impl Serialize<bool> for DefaultSerializer {
    type Output = ZBuf;

    fn serialize(self, t: bool) -> Self::Output {
        // SAFETY: casting a bool into an integer is well-defined behaviour.
        //      0 is false, 1 is true: https://doc.rust-lang.org/std/primitive.bool.html
        ZBuf::from((t as u8).to_le_bytes())
    }
}

impl Deserialize<bool> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<bool, Self::Error> {
        let p = v.contiguous();
        if p.len() != 1 {
            bail!("Decode error:: bool invalid length ({} != {})", p.len(), 1);
        }
        match p[0] {
            0 => Ok(false),
            1 => Ok(true),
            invalid => bail!("Decode error: bool invalid value ({})", invalid),
        }
    }
}

// - Zenoh advanced types encoders/decoders
// JSON
impl Serialize<&serde_json::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_json::Error>;

    fn serialize(self, t: &serde_json::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_json::to_writer(payload.writer(), t)?;
        Ok(payload)
    }
}

impl Serialize<serde_json::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_json::Error>;

    fn serialize(self, t: serde_json::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<serde_json::Value> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<serde_json::Value, Self::Error> {
        serde_json::from_reader(v.reader()).map_err(|e| zerror!("{}", e))
    }
}

// CBOR
impl Serialize<&serde_cbor::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_cbor::Error>;

    fn serialize(self, t: &serde_cbor::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_cbor::to_writer(payload.writer(), t)?;
        Ok(payload)
    }
}

impl Serialize<serde_cbor::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_cbor::Error>;

    fn serialize(self, t: serde_cbor::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<serde_cbor::Value> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<serde_cbor::Value, Self::Error> {
        serde_cbor::from_reader(v.reader()).map_err(|e| zerror!("{}", e))
    }
}

// Pickle
impl Serialize<&serde_pickle::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_pickle::Error>;

    fn serialize(self, t: &serde_pickle::Value) -> Self::Output {
        let mut payload = Payload::empty();
        serde_pickle::value_to_writer(
            &mut payload.writer(),
            t,
            serde_pickle::SerOptions::default(),
        )?;
        Ok(payload)
    }
}

impl Serialize<serde_pickle::Value> for DefaultSerializer {
    type Output = Result<Payload, serde_pickle::Error>;

    fn serialize(self, t: serde_pickle::Value) -> Self::Output {
        Self.serialize(&t)
    }
}

impl Deserialize<serde_pickle::Value> for DefaultSerializer {
    type Error = ZError;

    fn deserialize(self, v: &Payload) -> Result<serde_pickle::Value, Self::Error> {
        serde_pickle::value_from_reader(v.reader(), serde_pickle::DeOptions::default())
            .map_err(|e| zerror!("{}", e))
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Serialize<Arc<SharedMemoryBuf>> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: Arc<SharedMemoryBuf>) -> Self::Output {
        Payload::new(t)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<Box<SharedMemoryBuf>> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: Box<SharedMemoryBuf>) -> Self::Output {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self.serialize(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl Serialize<SharedMemoryBuf> for DefaultSerializer {
    type Output = Payload;

    fn serialize(self, t: SharedMemoryBuf) -> Self::Output {
        Payload::new(t)
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

/// Default encoding mapping used by the [`DefaultSerializer`]. Please note that Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like. The mapping
/// here below is provided for user convenience and does its best to cover the most common
/// cases. To implement a custom mapping refer to [`EncodingMapping`] trait.

/// Default [`Encoding`] provided with Zenoh to facilitate the encoding and decoding
/// of [`Value`]s in the Rust API. Please note that Zenoh does not impose any
/// encoding and users are free to use any encoder they like. [`DefaultSerializer`]
/// is simply provided as convenience to the users.

///
/// For compatibility purposes Zenoh reserves any prefix value from `0` to `1023` included.
/// Any application is free to use any value from `1024` to `65535`.
#[derive(Clone, Copy, Debug)]
pub struct DefaultSerializer;

/// Provide some facilities specific to the Rust API to encode/decode a [`Value`] with an `Serialize`.
impl Payload {
    /// Encode an object of type `T` as a [`Value`] using the [`DefaultSerializer`].
    ///
    /// ```rust
    /// use zenoh::value::Value;
    ///
    /// let start = String::from("abc");
    /// let payload = Payload::serialize(start.clone());
    /// let end: String = payload.decode().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn serialize<T>(t: T) -> Self
    where
        DefaultSerializer: Serialize<T, Output = Payload>,
    {
        Self::serialize_with(t, DefaultSerializer)
    }

    /// Encode an object of type `T` as a [`Value`] using a provided [`Serialize`].
    ///
    /// ```rust
    /// use zenoh::prelude::sync::*;
    /// use zenoh_result::{self, zerror, ZError, ZResult};
    /// use zenoh::encoding::{Serialize, Deserialize};
    ///
    /// struct MySerialize;
    ///
    /// impl MySerialize {
    ///     pub const STRING: Encoding = Encoding::new(2);   
    /// }
    ///
    /// impl Serialize<String> for MySerialize {
    ///     type Output = Value;
    ///
    ///     fn serialize(self, s: String) -> Self::Output {
    ///         Value::new(s.into_bytes().into()).with_encoding(MySerialize::STRING)
    ///     }
    /// }
    ///
    /// impl Deserialize<String> for MySerialize {
    ///     type Error = ZError;
    ///
    ///     fn deserialize(self, v: &Value) -> Result<String, Self::Error> {
    ///         if v.encoding == MySerialize::STRING {
    ///             String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e))
    ///         } else {
    ///             Err(zerror!("Invalid encoding"))
    ///         }
    ///     }
    /// }
    ///
    /// let start = String::from("abc");
    /// let payload = Payload::serialize_with(start.clone(), MySerialize);
    /// let end: String = value.decode_with(MySerialize).unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn serialize_with<T, M>(t: T, m: M) -> Self
    where
        M: Serialize<T, Output = Payload>,
    {
        m.serialize(t)
    }

    /// Decode an object of type `T` from a [`Value`] using the [`DefaultSerializer`].
    /// See [encode](Value::encode) for an example.
    pub fn deserialize<T>(&self) -> ZResult<T>
    where
        DefaultSerializer: Deserialize<T>,
        <DefaultSerializer as Deserialize<T>>::Error: Debug,
    {
        self.deserialize_with(DefaultSerializer)
    }

    /// Decode an object of type `T` from a [`Value`] using a provided [`Serialize`].
    /// See [encode_with](Value::encode_with) for an example.
    pub fn deserialize_with<T, M>(&self, m: M) -> ZResult<T>
    where
        M: Deserialize<T>,
        <M as Deserialize<T>>::Error: Debug,
    {
        let t: T = m.deserialize(self).map_err(|e| zerror!("{:?}", e))?;
        Ok(t)
    }
}

impl From<Payload> for ZBuf {
    fn from(value: Payload) -> Self {
        value.0
    }
}

impl From<&Payload> for ZBuf {
    fn from(value: &Payload) -> Self {
        value.0.clone()
    }
}

impl<T> From<T> for Payload
where
    DefaultSerializer: Serialize<T, Output = Payload>,
{
    fn from(t: T) -> Self {
        DefaultSerializer.serialize(t)
    }
}

// For convenience to always convert a Value the examples
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringOrBase64 {
    String(String),
    Base64(String),
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

impl From<Payload> for StringOrBase64 {
    fn from(v: Payload) -> Self {
        use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
        match v.deserialize::<String>() {
            Ok(s) => StringOrBase64::String(s),
            Err(_) => StringOrBase64::Base64(b64_std_engine.encode(v.contiguous())),
        }
    }
}
