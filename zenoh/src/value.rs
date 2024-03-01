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

//! Value primitives.
use std::ops::Deref;

use crate::buffers::ZBuf;
use crate::encoding::{Decoder, DefaultEncoding, Encoder};
use crate::prelude::Encoding;
use zenoh_buffers::buffer::SplitBuffer;
use zenoh_result::{ZError, ZResult};

/// A zenoh [`Value`] contains a `payload` and an [`Encoding`] that indicates how the `payload`
/// should be interpreted.
#[non_exhaustive]
#[derive(Clone)]
pub struct Value {
    /// The payload of this [`Value`].
    pub payload: ZBuf,
    /// An [`Encoding`] description indicating how the payload should be interpreted.
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new [`Value`]. The enclosed [`Encoding`] is [empty](`Encoding::empty`) by default if
    /// not specified via the [`encoding`](`Value::encoding`).
    pub fn new(payload: ZBuf) -> Self {
        Value {
            payload,
            encoding: Encoding::DEFAULT,
        }
    }

    /// Creates an empty [`Value`].
    pub const fn empty() -> Self {
        Value {
            payload: ZBuf::empty(),
            encoding: Encoding::DEFAULT,
        }
    }

    /// Sets the payload of this [`Value`]`.
    #[inline(always)]
    pub fn payload(mut self, payload: ZBuf) -> Self {
        self.payload = payload;
        self
    }

    /// Sets the encoding of this [`Value`]`.
    #[inline(always)]
    pub fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }
}

/// Provide some facilities specific to the Rust API to encode/decode a [`Value`] with an `Encoder`.
impl Value {
    /// Encode an object of type `T` as a [`Value`] using the [`DefaultEncoding`].
    ///
    /// ```rust
    /// use zenoh::value::Value;
    ///
    /// let start = String::from("abc");
    /// let value = Value::encode(start.clone());
    /// let end: String = value.decode().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn encode<T>(t: T) -> Self
    where
        DefaultEncoding: Encoder<T, Output = ZBuf>,
    {
        Value {
            payload: DefaultEncoding.encode(t),
            encoding: Encoding::empty(),
        }
    }

    /// Encode an object of type `T` as a [`Value`] using a provided [`Encoder`].
    ///
    /// ```rust
    /// use zenoh::prelude::sync::*;
    /// use zenoh_result::{self, zerror, ZError, ZResult};
    /// use zenoh::encoding::{Encoder, Decoder};
    ///
    /// struct MyEncoder;
    ///
    /// impl MyEncoder {
    ///     pub const STRING: Encoding = Encoding::new(2);   
    /// }
    ///
    /// impl Encoder<String> for MyEncoder {
    ///     type Output = Value;
    ///
    ///     fn encode(self, s: String) -> Self::Output {
    ///         Value::new(s.into_bytes().into()).with_encoding(MyEncoder::STRING)
    ///     }
    /// }
    ///
    /// impl Decoder<String> for MyEncoder {
    ///     type Error = ZError;
    ///
    ///     fn decode(self, v: &Value) -> Result<String, Self::Error> {
    ///         if v.encoding == MyEncoder::STRING {
    ///             String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e))
    ///         } else {
    ///             Err(zerror!("Invalid encoding"))
    ///         }
    ///     }
    /// }
    ///
    /// let start = String::from("abc");
    /// let value = Value::encode_with(start.clone(), MyEncoder);
    /// let end: String = value.decode_with(MyEncoder).unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn encode_with<T, M>(t: T, m: M) -> Self
    where
        M: Encoder<T, Output = ZBuf>,
    {
        Value {
            payload: m.encode(t),
            encoding: Encoding::empty(),
        }
    }

    /// Decode an object of type `T` from a [`Value`] using the [`DefaultEncoding`].
    /// See [encode](Value::encode) for an example.
    pub fn decode<T>(&self) -> ZResult<T>
    where
        DefaultEncoding: Decoder<T, Error = ZError>,
    {
        Ok(DefaultEncoding.decode(&self.payload)?)
    }

    /// Decode an object of type `T` from a [`Value`] using a provided [`Encoder`].
    /// See [encode_with](Value::encode_with) for an example.
    pub fn decode_with<T, M>(&self, m: M) -> ZResult<T>
    where
        M: Decoder<T, Error = ZError>,
    {
        Ok(m.decode(&self.payload)?)
    }
}

/// Build a [`Value`] from any type `T` supported by the [`DefaultEncoding`].
/// This trait implementation is provided as convenience for users using the [`DefaultEncoding`].
impl<T> From<T> for Value
where
    DefaultEncoding: Encoder<T, Output = ZBuf>,
{
    fn from(t: T) -> Self {
        Value::encode(t)
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Value{{ payload: {:?}, encoding: {:?} }}",
            self.payload, self.encoding
        )
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

impl From<Value> for StringOrBase64 {
    fn from(v: Value) -> Self {
        use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
        match String::try_from(v.payload.clone()) {
            Ok(s) => StringOrBase64::String(s),
            Err(_) => StringOrBase64::Base64(b64_std_engine.encode(v.payload.contiguous())),
        }
    }
}
