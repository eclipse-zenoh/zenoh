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
    fn parse<S>(s: S) -> ZResult<Encoding>
    where
        S: Into<String>;
}

pub struct DefaultEncodingMapping;

impl DefaultEncodingMapping {
    pub const EMPTY: EncodingPrefix = 0;
    pub const APP_OCTET_STREAM: EncodingPrefix = 1;
    pub const APP_CUSTOM: EncodingPrefix = 2;
    pub const TEXT_PLAIN: EncodingPrefix = 3;
    pub const APP_PROPERTIES: EncodingPrefix = 4;
    pub const APP_JSON: EncodingPrefix = 5;
    pub const APP_SQL: EncodingPrefix = 6;
    pub const APP_INTEGER: EncodingPrefix = 7;
    pub const APP_FLOAT: EncodingPrefix = 8;
    pub const APP_XML: EncodingPrefix = 9;
    pub const APP_XHTML_XML: EncodingPrefix = 10;
    pub const APP_XWWW_FORM_URLENCODED: EncodingPrefix = 11;
    pub const TEXT_JSON: EncodingPrefix = 12;
    pub const TEXT_HTML: EncodingPrefix = 13;
    pub const TEXT_XML: EncodingPrefix = 14;
    pub const TEXT_CSS: EncodingPrefix = 15;
    pub const TEXT_CSV: EncodingPrefix = 16;
    pub const TEXT_JAVASCRIPT: EncodingPrefix = 17;
    pub const IMAGE_JPEG: EncodingPrefix = 18;
    pub const IMAGE_PNG: EncodingPrefix = 19;
    pub const IMAGE_GIF: EncodingPrefix = 20;

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

impl EncodingMapping for DefaultEncodingMapping {
    /// Given a numeric prefix ID returns its string representation
    fn prefix_to_str(&self, p: EncodingPrefix) -> &'static str {
        match Self::KNOWN_PREFIX.get(&p) {
            Some(p) => p,
            None => "unknown",
        }
    }

    /// Given the string representation of a prefix returns its prefix ID.
    /// Empty is returned in case the prefix is unknown.
    fn str_to_prefix(&self, s: &str) -> EncodingPrefix {
        match Self::KNOWN_STRING.get(s) {
            Some(p) => *p,
            None => Self::EMPTY,
        }
    }

    /// Parse a string into a valid Encoding. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input.
    fn parse<S>(t: S) -> ZResult<Encoding>
    where
        S: Into<String>,
    {
        fn _parse(mut t: String) -> ZResult<Encoding> {
            if t.is_empty() {
                return Ok(DefaultEncoding::EMPTY);
            }

            // Skip empty string mapping
            for (s, p) in DefaultEncodingMapping::KNOWN_STRING.entries().skip(1) {
                if let Some(i) = t.find(s) {
                    return Encoding::new(*p).with_suffix(t.split_off(i + s.len()));
                }
            }
            DefaultEncoding::EMPTY.with_suffix(t)
        }
        _parse(t.into())
    }
}

/// Default encoder provided with Zenoh to facilitate the encoding and decoding
/// of Values in the Rust API. Please note that Zenoh does not impose any
/// encoding and users are free to use any encoder they like.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefaultEncoding;

impl DefaultEncoding {
    pub const EMPTY: Encoding = Encoding::new(DefaultEncodingMapping::EMPTY);
    pub const APP_OCTET_STREAM: Encoding = Encoding::new(DefaultEncodingMapping::APP_OCTET_STREAM);
    pub const APP_CUSTOM: Encoding = Encoding::new(DefaultEncodingMapping::APP_CUSTOM);
    pub const TEXT_PLAIN: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_PLAIN);
    pub const APP_PROPERTIES: Encoding = Encoding::new(DefaultEncodingMapping::APP_PROPERTIES);
    pub const APP_JSON: Encoding = Encoding::new(DefaultEncodingMapping::APP_JSON);
    pub const APP_SQL: Encoding = Encoding::new(DefaultEncodingMapping::APP_SQL);
    pub const APP_INTEGER: Encoding = Encoding::new(DefaultEncodingMapping::APP_INTEGER);
    pub const APP_FLOAT: Encoding = Encoding::new(DefaultEncodingMapping::APP_FLOAT);
    pub const APP_XML: Encoding = Encoding::new(DefaultEncodingMapping::APP_XML);
    pub const APP_XHTML_XML: Encoding = Encoding::new(DefaultEncodingMapping::APP_XHTML_XML);
    pub const APP_XWWW_FORM_URLENCODED: Encoding =
        Encoding::new(DefaultEncodingMapping::APP_XWWW_FORM_URLENCODED);
    pub const TEXT_JSON: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_JSON);
    pub const TEXT_HTML: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_HTML);
    pub const TEXT_XML: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_XML);
    pub const TEXT_CSS: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_CSS);
    pub const TEXT_CSV: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_CSV);
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_JAVASCRIPT);
    pub const IMAGE_JPEG: Encoding = Encoding::new(DefaultEncodingMapping::IMAGE_JPEG);
    pub const IMAGE_PNG: Encoding = Encoding::new(DefaultEncodingMapping::IMAGE_PNG);
    pub const IMAGE_GIF: Encoding = Encoding::new(DefaultEncodingMapping::IMAGE_GIF);
}

// Encoder
pub trait Encoder<T> {
    fn encode(t: T) -> Value;
}

pub trait Decoder<T> {
    fn decode(t: &Value) -> ZResult<T>;
}

// Bytes conversion
impl Encoder<ZBuf> for DefaultEncoding {
    fn encode(t: ZBuf) -> Value {
        Value {
            payload: t,
            encoding: DefaultEncoding::APP_OCTET_STREAM,
        }
    }
}

impl Decoder<ZBuf> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<ZBuf> {
        if v.encoding == DefaultEncoding::APP_OCTET_STREAM {
            Ok(v.payload.clone())
        } else {
            Err(zerror!("{:?} can not be converted into String", v).into())
        }
    }
}

impl Encoder<Vec<u8>> for DefaultEncoding {
    fn encode(t: Vec<u8>) -> Value {
        Self::encode(ZBuf::from(t))
    }
}

impl Encoder<&[u8]> for DefaultEncoding {
    fn encode(t: &[u8]) -> Value {
        Self::encode(t.to_vec())
    }
}

impl Decoder<Vec<u8>> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<Vec<u8>> {
        let v: ZBuf = Self::decode(v)?;
        Ok(v.contiguous().to_vec())
    }
}

impl<'a> Encoder<Cow<'a, [u8]>> for DefaultEncoding {
    fn encode(t: Cow<'a, [u8]>) -> Value {
        Self::encode(ZBuf::from(t.to_vec()))
    }
}

impl<'a> Decoder<Cow<'a, [u8]>> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<Cow<'a, [u8]>> {
        let v: Vec<u8> = Self::decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// String
impl Encoder<String> for DefaultEncoding {
    fn encode(s: String) -> Value {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: DefaultEncoding::TEXT_PLAIN,
        }
    }
}

impl Encoder<&str> for DefaultEncoding {
    fn encode(s: &str) -> Value {
        Self::encode(s.to_string())
    }
}

impl Decoder<String> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<String> {
        if v.encoding == DefaultEncoding::TEXT_PLAIN {
            String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e).into())
        } else {
            Err(zerror!("{:?} can not be converted into String", v).into())
        }
    }
}

impl<'a> Encoder<Cow<'a, str>> for DefaultEncoding {
    fn encode(s: Cow<'a, str>) -> Value {
        Self::encode(s.to_string())
    }
}

impl<'a> Decoder<Cow<'a, str>> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<Cow<'a, str>> {
        let v: String = Self::decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// Sample
impl Encoder<Sample> for DefaultEncoding {
    fn encode(t: Sample) -> Value {
        t.value
    }
}

// Integers
macro_rules! impl_int {
    ($t:ty, $encoding:expr) => {
        impl Encoder<$t> for DefaultEncoding {
            fn encode(t: $t) -> Value {
                Value {
                    payload: ZBuf::from(t.to_string().into_bytes()),
                    encoding: $encoding,
                }
            }
        }

        impl Encoder<&$t> for DefaultEncoding {
            fn encode(t: &$t) -> Value {
                Self::encode(*t)
            }
        }

        impl Decoder<$t> for DefaultEncoding {
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

impl_int!(u8, DefaultEncoding::APP_INTEGER);
impl_int!(u16, DefaultEncoding::APP_INTEGER);
impl_int!(u32, DefaultEncoding::APP_INTEGER);
impl_int!(u64, DefaultEncoding::APP_INTEGER);
impl_int!(usize, DefaultEncoding::APP_INTEGER);

impl_int!(i8, DefaultEncoding::APP_INTEGER);
impl_int!(i16, DefaultEncoding::APP_INTEGER);
impl_int!(i32, DefaultEncoding::APP_INTEGER);
impl_int!(i64, DefaultEncoding::APP_INTEGER);
impl_int!(isize, DefaultEncoding::APP_INTEGER);

// Floats
impl_int!(f32, DefaultEncoding::APP_FLOAT);
impl_int!(f64, DefaultEncoding::APP_FLOAT);

// JSON
impl Encoder<&serde_json::Value> for DefaultEncoding {
    fn encode(t: &serde_json::Value) -> Value {
        Value {
            payload: ZBuf::from(t.to_string().into_bytes()),
            encoding: DefaultEncoding::APP_JSON,
        }
    }
}

impl Encoder<serde_json::Value> for DefaultEncoding {
    fn encode(t: serde_json::Value) -> Value {
        Self::encode(&t)
    }
}

impl Decoder<serde_json::Value> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<serde_json::Value> {
        if v.encoding == DefaultEncoding::APP_JSON || v.encoding == DefaultEncoding::TEXT_JSON {
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
impl Encoder<&Properties> for DefaultEncoding {
    fn encode(t: &Properties) -> Value {
        Value {
            payload: ZBuf::from(t.to_string().into_bytes()),
            encoding: DefaultEncoding::APP_PROPERTIES,
        }
    }
}

impl Encoder<Properties> for DefaultEncoding {
    fn encode(t: Properties) -> Value {
        Self::encode(&t)
    }
}

impl Decoder<Properties> for DefaultEncoding {
    fn decode(v: &Value) -> ZResult<Properties> {
        if v.encoding == DefaultEncoding::APP_PROPERTIES {
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
impl Encoder<Arc<SharedMemoryBuf>> for DefaultEncoding {
    fn encode(t: Arc<SharedMemoryBuf>) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoding::APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<Box<SharedMemoryBuf>> for DefaultEncoding {
    fn encode(t: Box<SharedMemoryBuf>) -> Value {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self::encode(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<SharedMemoryBuf> for DefaultEncoding {
    fn encode(t: SharedMemoryBuf) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoding::APP_OCTET_STREAM,
        }
    }
}

mod tests {
    #[test]
    fn encoder() {
        use crate::Value;
        use zenoh_buffers::ZBuf;
        use zenoh_collections::Properties;

        macro_rules! encode_decode {
            ($t:ty, $in:expr) => {
                let t = $in.clone();
                let v = Value::encode(t);
                let out: $t = v.decode().unwrap();
                assert_eq!($in, out)
            };
        }

        encode_decode!(u8, u8::MIN);
        encode_decode!(u16, u16::MIN);
        encode_decode!(u32, u32::MIN);
        encode_decode!(u64, u64::MIN);
        encode_decode!(usize, usize::MIN);

        encode_decode!(u8, u8::MAX);
        encode_decode!(u16, u16::MAX);
        encode_decode!(u32, u32::MAX);
        encode_decode!(u64, u64::MAX);
        encode_decode!(usize, usize::MAX);

        encode_decode!(i8, i8::MIN);
        encode_decode!(i16, i16::MIN);
        encode_decode!(i32, i32::MIN);
        encode_decode!(i64, i64::MIN);
        encode_decode!(isize, isize::MIN);

        encode_decode!(i8, i8::MAX);
        encode_decode!(i16, i16::MAX);
        encode_decode!(i32, i32::MAX);
        encode_decode!(i64, i64::MAX);
        encode_decode!(isize, isize::MAX);

        encode_decode!(f32, f32::MIN);
        encode_decode!(f64, f64::MIN);

        encode_decode!(f32, f32::MAX);
        encode_decode!(f64, f64::MAX);

        encode_decode!(String, "");
        encode_decode!(String, String::from("abcdefghijklmnopqrstuvwxyz"));

        encode_decode!(Vec<u8>, vec![0u8; 0]);
        encode_decode!(Vec<u8>, vec![0u8; 64]);

        encode_decode!(ZBuf, ZBuf::from(vec![0u8; 0]));
        encode_decode!(ZBuf, ZBuf::from(vec![0u8; 64]));

        encode_decode!(Properties, Properties::from(""));
        encode_decode!(Properties, Properties::from("a=1;b=2"));
    }
}
