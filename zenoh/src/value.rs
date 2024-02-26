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
use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
use zenoh_result::ZResult;

use crate::buffers::ZBuf;
use crate::encoding::{Decoder, DefaultEncoding, Encoder};
use crate::prelude::{Encoding, SplitBuffer};

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
            encoding: Encoding::empty(),
        }
    }

    /// Creates an empty [`Value`].
    pub fn empty() -> Self {
        Value {
            payload: ZBuf::empty(),
            encoding: Encoding::empty(),
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
        DefaultEncoding: Encoder<T>,
    {
        DefaultEncoding.encode(t)
    }

    /// Encode an object of type `T` as a [`Value`] using a provided [`Encoder`].
    ///
    /// ```rust
    /// use zenoh::prelude::sync::*;
    /// use zenoh_result::{zerror, ZResult};
    /// use zenoh::encoding::{Encoder, Decoder};
    ///
    /// struct MyEncoder;
    ///
    /// impl MyEncoder {
    ///     pub const STRING: Encoding = Encoding::new(2);   
    /// }
    ///
    /// impl Encoder<String> for MyEncoder {
    ///     fn encode(s: String) -> Value {
    ///         Value::new(s.into_bytes().into()).encoding(MyEncoder::STRING)
    ///     }
    /// }
    ///
    /// impl Decoder<String> for MyEncoder {
    ///     fn decode(v: &Value) -> ZResult<String> {
    ///         if v.encoding == MyEncoder::STRING {
    ///             String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e).into())
    ///         } else {
    ///             Err(zerror!("Invalid encoding").into())
    ///         }
    ///     }
    /// }
    ///
    /// let start = String::from("abc");
    /// let value = Value::encode_with::<MyEncoder, _>(start.clone());
    /// let end: String = value.decode_with::<MyEncoder, _>().unwrap();
    /// assert_eq!(start, end);
    /// ```
    pub fn encode_with<T, M>(t: T, m: M) -> Self
    where
        M: Encoder<T>,
    {
        m.encode(t)
    }

    /// Decode an object of type `T` from a [`Value`] using the [`DefaultEncoding`].
    /// See [encode](Value::encode) for an example.
    pub fn decode<T>(&self) -> ZResult<T>
    where
        DefaultEncoding: Decoder<T>,
    {
        DefaultEncoding.decode(self)
    }

    /// Decode an object of type `T` from a [`Value`] using a provided [`Encoder`].
    /// See [encode_with](Value::encode_with) for an example.
    pub fn decode_with<T, M>(&self, m: M) -> ZResult<T>
    where
        M: Decoder<T>,
    {
        m.decode(self)
    }
}

/// Build a [`Value`] from any type `T` supported by the [`DefaultEncoding`].
/// This trait implementation is provided as convenience for users using the [`DefaultEncoding`].
impl<T> From<T> for Value
where
    DefaultEncoding: Encoder<T>,
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

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let payload = self.payload.contiguous();
        match std::str::from_utf8(&payload) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => {
                let s = b64_std_engine.encode(&payload);
                write!(f, "{}", s)
            }
        }
    }
}

impl std::error::Error for Value {}
