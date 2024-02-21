//
// Copyright (c) 2024 ZettaScale Technology
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
use crate::{value::Value, Sample};
use phf::phf_ordered_map;
use std::borrow::Cow;
#[cfg(feature = "shared-memory")]
use std::sync::Arc;
use zenoh_buffers::{buffer::SplitBuffer, ZBuf};
use zenoh_collections::Properties;
use zenoh_protocol::core::{Encoding, EncodingPrefix};
use zenoh_result::ZResult;
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

pub trait EncodingMapping {
    fn prefix_to_str(&self, e: EncodingPrefix) -> &str;
    fn str_to_prefix(&self, s: &str) -> EncodingPrefix;
    fn parse(s: &str) -> ZResult<Encoding>;
}

pub trait Encoder<T> {
    fn encode(t: T) -> Value;
}

pub trait Decoder<T> {
    fn decode(t: &Value) -> ZResult<T>;
}

/// Default encoder provided with Zenoh to facilitate the encoding and decoding
/// of Values in the Rust API. Please note that Zenoh does not impose any
/// encoding and users are free to use any encoder they like.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefaultEncoder;

impl DefaultEncoder {
    pub const EMPTY: Encoding = Encoding::new(0);
    pub const APP_OCTET_STREAM: Encoding = Encoding::new(1);
    pub const APP_CUSTOM: Encoding = Encoding::new(2);
    pub const TEXT_PLAIN: Encoding = Encoding::new(3);
    pub const APP_PROPERTIES: Encoding = Encoding::new(4);
    pub const APP_JSON: Encoding = Encoding::new(5);
    pub const APP_SQL: Encoding = Encoding::new(6);
    pub const APP_INTEGER: Encoding = Encoding::new(7);
    pub const APP_FLOAT: Encoding = Encoding::new(8);
    pub const APP_XML: Encoding = Encoding::new(9);
    pub const APP_XHTML_XML: Encoding = Encoding::new(10);
    pub const APP_XWWW_FORM_URLENCODED: Encoding = Encoding::new(11);
    pub const TEXT_JSON: Encoding = Encoding::new(12);
    pub const TEXT_HTML: Encoding = Encoding::new(13);
    pub const TEXT_XML: Encoding = Encoding::new(14);
    pub const TEXT_CSS: Encoding = Encoding::new(15);
    pub const TEXT_CSV: Encoding = Encoding::new(16);
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::new(17);
    pub const IMAGE_JPEG: Encoding = Encoding::new(18);
    pub const IMAGE_PNG: Encoding = Encoding::new(19);
    pub const IMAGE_GIF: Encoding = Encoding::new(20);

    // A perfect hashmap for fast lookup of prefixes
    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        0u64 =>  "",
        1u64 =>  "application/octet-stream",
        2u64 =>  "application/custom", // non iana standard
        3u64 =>  "text/plain",
        4u64 =>  "application/properties", // non iana standard
        5u64 =>  "application/json", // if not readable from casual users
        6u64 =>  "application/sql",
        7u64 =>  "application/integer", // non iana standard
        8u64 =>  "application/float", // non iana standard
        9u64 =>  "application/xml", // if not readable from casual users (RFC 3023, sec 3)
        10u64 => "application/xhtml+xml",
        11u64 => "application/x-www-form-urlencoded",
        12u64 => "text/json", // non iana standard - if readable from casual users
        13u64 => "text/html",
        14u64 => "text/xml", // if readable from casual users (RFC 3023, section 3)
        15u64 => "text/css",
        16u64 => "text/csv",
        17u64 => "text/javascript",
        18u64 => "image/jpeg",
        19u64 => "image/png",
        20u64 => "image/gif",
    };

    // A perfect hashmap for fast lookup of prefixes
    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "" => 0u64,
        "application/octet-stream" => 1u64,
        "application/custom" => 2u64, // non iana standard
        "text/plain" => 3u64,
        "application/properties" => 4u64, // non iana standard
        "application/json" => 5u64, // if not readable from casual users
        "application/sql" => 6u64,
        "application/integer" => 7u64, // non iana standard
        "application/float" => 8u64, // non iana standard
        "application/xml" => 9u64, // if not readable from casual users (RFC 3023, sec 3)
        "application/xhtml+xml" => 10u64,
        "application/x-www-form-urlencoded" => 11u64,
        "text/json" => 12u64, // non iana standard - if readable from casual users
        "text/html" => 13u64,
        "text/xml" => 14u64, // if readable from casual users (RFC 3023, section 3)
        "text/css" => 15u64,
        "text/csv" => 16u64,
        "text/javascript" => 17u64,
        "image/jpeg" => 18u64,
        "image/png" => 19u64,
        "image/gif" => 20u64,
    };
}

impl EncodingMapping for DefaultEncoder {
    // Given a numeric prefix ID returns its string representation
    fn prefix_to_str(&self, p: zenoh_protocol::core::EncodingPrefix) -> &'static str {
        match Self::KNOWN_PREFIX.get(&p) {
            Some(p) => p,
            None => "unknown",
        }
    }

    // Given the string representation of a prefix returns its prefix ID.
    // Empty is returned in case the prefix is unknown.
    fn str_to_prefix(&self, s: &str) -> zenoh_protocol::core::EncodingPrefix {
        match Self::KNOWN_STRING.get(s) {
            Some(p) => *p,
            None => Self::EMPTY.prefix(),
        }
    }

    // Parse a string into a valid Encoding. This functions performs the necessary
    // prefix mapping and suffix substring when parsing the input.
    fn parse(t: &str) -> ZResult<Encoding> {
        for (s, p) in Self::KNOWN_STRING.entries() {
            if let Some((_, b)) = t.split_once(s) {
                return Encoding::new(*p).with_suffix(b.to_string());
            }
        }
        Encoding::empty().with_suffix(t.to_string())
    }
}

// -- impls

// Bytes conversion
impl Encoder<ZBuf> for DefaultEncoder {
    fn encode(t: ZBuf) -> Value {
        Value {
            payload: t,
            encoding: DefaultEncoder::APP_OCTET_STREAM,
        }
    }
}

impl Decoder<ZBuf> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<ZBuf> {
        if v.encoding == DefaultEncoder::APP_OCTET_STREAM {
            Ok(v.payload.clone())
        } else {
            Err(zerror!("{:?} can not be converted into String", v).into())
        }
    }
}

impl Encoder<Vec<u8>> for DefaultEncoder {
    fn encode(t: Vec<u8>) -> Value {
        Self::encode(ZBuf::from(t))
    }
}

impl Encoder<&[u8]> for DefaultEncoder {
    fn encode(t: &[u8]) -> Value {
        Self::encode(t.to_vec())
    }
}

impl Decoder<Vec<u8>> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<Vec<u8>> {
        let v: ZBuf = Self::decode(v)?;
        Ok(v.contiguous().to_vec())
    }
}

impl<'a> Encoder<Cow<'a, [u8]>> for DefaultEncoder {
    fn encode(t: Cow<'a, [u8]>) -> Value {
        Self::encode(ZBuf::from(t.to_vec()))
    }
}

impl<'a> Decoder<Cow<'a, [u8]>> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<Cow<'a, [u8]>> {
        let v: Vec<u8> = Self::decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// String
impl Encoder<String> for DefaultEncoder {
    fn encode(s: String) -> Value {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: DefaultEncoder::TEXT_PLAIN,
        }
    }
}

impl Encoder<&str> for DefaultEncoder {
    fn encode(s: &str) -> Value {
        Self::encode(s.to_string())
    }
}

impl Decoder<String> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<String> {
        if v.encoding == DefaultEncoder::TEXT_PLAIN {
            String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e).into())
        } else {
            Err(zerror!("{:?} can not be converted into String", v).into())
        }
    }
}

impl<'a> Encoder<Cow<'a, str>> for DefaultEncoder {
    fn encode(s: Cow<'a, str>) -> Value {
        Self::encode(s.to_string())
    }
}

impl<'a> Decoder<Cow<'a, str>> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<Cow<'a, str>> {
        let v: String = Self::decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// Sample
impl Encoder<Sample> for DefaultEncoder {
    fn encode(t: Sample) -> Value {
        t.value
    }
}

// Integers
macro_rules! impl_int {
    ($t:ty, $encoding:expr) => {
        impl Encoder<$t> for DefaultEncoder {
            fn encode(t: $t) -> Value {
                Value {
                    payload: ZBuf::from(t.to_string().into_bytes()),
                    encoding: $encoding,
                }
            }
        }

        impl Encoder<&$t> for DefaultEncoder {
            fn encode(t: &$t) -> Value {
                Self::encode(*t)
            }
        }

        impl Decoder<$t> for DefaultEncoder {
            fn decode(v: &Value) -> ZResult<$t> {
                if v.encoding == $encoding {
                    let v: $t = std::str::from_utf8(&v.payload.contiguous())
                        .map_err(|e| zerror!("{}", e))?
                        .parse()
                        .map_err(|e| zerror!("{}", e))?;
                    Ok(v)
                } else {
                    Err(zerror!("{:?} can not be converted into String", v).into())
                }
            }
        }
    };
}

impl_int!(u8, DefaultEncoder::APP_INTEGER);
impl_int!(u16, DefaultEncoder::APP_INTEGER);
impl_int!(u32, DefaultEncoder::APP_INTEGER);
impl_int!(u64, DefaultEncoder::APP_INTEGER);
impl_int!(usize, DefaultEncoder::APP_INTEGER);

impl_int!(i8, DefaultEncoder::APP_INTEGER);
impl_int!(i16, DefaultEncoder::APP_INTEGER);
impl_int!(i32, DefaultEncoder::APP_INTEGER);
impl_int!(i64, DefaultEncoder::APP_INTEGER);
impl_int!(isize, DefaultEncoder::APP_INTEGER);

// Floats
impl_int!(f32, DefaultEncoder::APP_FLOAT);
impl_int!(f64, DefaultEncoder::APP_FLOAT);

// JSON
impl Encoder<&serde_json::Value> for DefaultEncoder {
    fn encode(t: &serde_json::Value) -> Value {
        Value {
            payload: ZBuf::from(t.to_string().into_bytes()),
            encoding: DefaultEncoder::APP_JSON,
        }
    }
}

impl Encoder<serde_json::Value> for DefaultEncoder {
    fn encode(t: serde_json::Value) -> Value {
        Self::encode(&t)
    }
}

impl Decoder<serde_json::Value> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<serde_json::Value> {
        if v.encoding == DefaultEncoder::APP_JSON || v.encoding == DefaultEncoder::TEXT_JSON {
            let r: serde_json::Value = serde::Deserialize::deserialize(
                &mut serde_json::Deserializer::from_slice(&v.payload.contiguous()),
            )
            .map_err(|e| zerror!("{}", e))?;
            Ok(r)
        } else {
            Err(zerror!("{:?} can not be converted into JSON", v.encoding).into())
        }
    }
}

// Properties
impl Encoder<&Properties> for DefaultEncoder {
    fn encode(t: &Properties) -> Value {
        Value {
            payload: ZBuf::from(t.to_string().into_bytes()),
            encoding: DefaultEncoder::APP_PROPERTIES,
        }
    }
}

impl Encoder<Properties> for DefaultEncoder {
    fn encode(t: Properties) -> Value {
        Self::encode(&t)
    }
}

impl Decoder<Properties> for DefaultEncoder {
    fn decode(v: &Value) -> ZResult<Properties> {
        if v.encoding == DefaultEncoder::APP_PROPERTIES {
            let ps = Properties::from(
                std::str::from_utf8(&v.payload.contiguous()).map_err(|e| zerror!("{}", e))?,
            );
            Ok(ps)
        } else {
            Err(zerror!("{:?} can not be converted into Properties", v.encoding).into())
        }
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Encoder<Arc<SharedMemoryBuf>> for DefaultEncoder {
    fn encode(t: Arc<SharedMemoryBuf>) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoder::APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<Box<SharedMemoryBuf>> for DefaultEncoder {
    fn encode(t: Box<SharedMemoryBuf>) -> Value {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self::encode(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<SharedMemoryBuf> for DefaultEncoder {
    fn encode(t: SharedMemoryBuf) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoder::APP_OCTET_STREAM,
        }
    }
}
