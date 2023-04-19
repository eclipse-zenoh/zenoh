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
use std::borrow::Cow;
use std::convert::TryFrom;
#[cfg(feature = "shared-memory")]
use std::sync::Arc;

use zenoh_cfg_properties::Properties;
use zenoh_result::ZError;

use crate::buffers::ZBuf;
use crate::prelude::{Encoding, KnownEncoding, Sample, SplitBuffer};
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

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
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }

    /// Creates an empty Value.
    pub fn empty() -> Self {
        Value {
            payload: ZBuf::default(),
            encoding: KnownEncoding::AppOctetStream.into(),
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

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for Value {
    fn from(smb: Arc<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for Value {
    fn from(smb: Box<SharedMemoryBuf>) -> Self {
        let smb: Arc<SharedMemoryBuf> = smb.into();
        Self::from(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for Value {
    fn from(smb: SharedMemoryBuf) -> Self {
        Value {
            payload: smb.into(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

// Bytes conversion
impl From<ZBuf> for Value {
    fn from(buf: ZBuf) -> Self {
        Value {
            payload: buf,
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

impl TryFrom<&Value> for ZBuf {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.clone()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Cow<'a, [u8]>",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for ZBuf {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

impl From<&[u8]> for Value {
    fn from(buf: &[u8]) -> Self {
        Value::from(ZBuf::from(buf.to_vec()))
    }
}

impl<'a> TryFrom<&'a Value> for Cow<'a, [u8]> {
    type Error = ZError;

    fn try_from(v: &'a Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.contiguous()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Cow<'a, [u8]>",
                unexpected
            )),
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(buf: Vec<u8>) -> Self {
        Value::from(ZBuf::from(buf))
    }
}

impl TryFrom<&Value> for Vec<u8> {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.contiguous().to_vec()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Vec<u8>",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for Vec<u8> {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// String conversion
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: KnownEncoding::TextPlain.into(),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(s)),
            encoding: KnownEncoding::TextPlain.into(),
        }
    }
}

impl TryFrom<&Value> for String {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::TextPlain => {
                String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e))
            }
            unexpected => Err(zerror!("{:?} can not be converted into String", unexpected)),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// Sample conversion
impl From<Sample> for Value {
    fn from(s: Sample) -> Self {
        s.value
    }
}

// i64 conversion
impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(i.to_string())),
            encoding: KnownEncoding::AppInteger.into(),
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppInteger => std::str::from_utf8(&v.payload.contiguous())
                .map_err(|e| zerror!("{}", e))?
                .parse()
                .map_err(|e| zerror!("{}", e)),
            unexpected => Err(zerror!("{:?} can not be converted into i64", unexpected)),
        }
    }
}

impl TryFrom<Value> for i64 {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// f64 conversion
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value {
            payload: ZBuf::from(f.to_string().as_bytes().to_vec()),
            encoding: KnownEncoding::AppFloat.into(),
        }
    }
}

impl TryFrom<&Value> for f64 {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppFloat => std::str::from_utf8(&v.payload.contiguous())
                .map_err(|e| zerror!("{}", e))?
                .parse()
                .map_err(|e| zerror!("{}", e)),
            unexpected => Err(zerror!("{:?} can not be converted into f64", unexpected)),
        }
    }
}

impl TryFrom<Value> for f64 {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// JSON conversion
impl From<&serde_json::Value> for Value {
    fn from(json: &serde_json::Value) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(json.to_string())),
            encoding: KnownEncoding::AppJson.into(),
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        Value::from(&json)
    }
}

impl TryFrom<&Value> for serde_json::Value {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppJson | KnownEncoding::TextJson => {
                let r = serde::Deserialize::deserialize(&mut serde_json::Deserializer::from_slice(
                    &v.payload.contiguous(),
                ));
                r.map_err(|e| zerror!("{}", e))
            }
            unexpected => Err(zerror!(
                "{:?} can not be converted into Properties",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for serde_json::Value {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// Properties conversion
impl From<Properties> for Value {
    fn from(p: Properties) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(p.to_string())),
            encoding: KnownEncoding::AppProperties.into(),
        }
    }
}

impl TryFrom<&Value> for Properties {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match *v.encoding.prefix() {
            KnownEncoding::AppProperties => Ok(Properties::from(
                std::str::from_utf8(&v.payload.contiguous()).map_err(|e| zerror!("{}", e))?,
            )),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Properties",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for Properties {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}
