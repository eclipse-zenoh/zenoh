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
use crate::encoding::{Decoder, DefaultEncoder, Encoder};
use crate::prelude::{Encoding, SplitBuffer};

/// A zenoh Value.
#[non_exhaustive]
#[derive(Clone)]
pub struct Value {
    /// The payload of this Value.
    pub payload: ZBuf,
    /// An encoding description indicating how the associated payload is encoded.
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new zenoh Value.
    pub fn new(payload: ZBuf) -> Self {
        Value {
            payload,
            encoding: Encoding::empty(),
        }
    }

    /// Creates an empty Value.
    pub fn empty() -> Self {
        Value {
            payload: ZBuf::empty(),
            encoding: Encoding::empty(),
        }
    }

    /// Sets the encoding of this zenoh Value.
    #[inline(always)]
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Value{{ payload: {:?}, encoding: {} }}",
            self.payload, self.encoding
        )
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let payload = self.payload.contiguous();
        write!(
            f,
            "{}",
            String::from_utf8(payload.clone().into_owned())
                .unwrap_or_else(|_| b64_std_engine.encode(payload))
        )
    }
}

impl std::error::Error for Value {}

impl Value {
    pub fn decode<T>(&self) -> ZResult<T>
    where
        DefaultEncoder: Decoder<T>,
    {
        DefaultEncoder::decode(self)
    }

    pub fn decode_with<T, M>(&self) -> ZResult<T>
    where
        M: Decoder<T>,
    {
        M::decode(self)
    }

    pub fn encode<T>(t: T) -> Self
    where
        DefaultEncoder: Encoder<T>,
    {
        DefaultEncoder::encode(t)
    }

    pub fn encode_with<T, M>(t: T) -> Self
    where
        M: Encoder<T>,
    {
        M::encode(t)
    }
}

impl<T> From<T> for Value
where
    DefaultEncoder: Encoder<T>,
{
    fn from(t: T) -> Self {
        Value::encode(t)
    }
}
