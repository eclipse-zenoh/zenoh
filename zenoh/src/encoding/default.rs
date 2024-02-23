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
use crate::{
    encoding::{Decoder, Encoder, EncodingMapping},
    value::Value,
    Sample,
};
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

/// Default encoding mapping used by the [`DefaultEncoding`]. Please note that Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like. The mapping
/// here below is provided for user convenience and does its best to cover the most common
/// cases. To implement a custom mapping refer to [`EncodingMapping`] trait.
#[derive(Clone, Copy, Debug)]
pub struct DefaultEncodingMapping;

impl DefaultEncodingMapping {
    /// Unspecified [`EncodingPrefix`].
    /// Note that an [`Encoding`] could have an empty [prefix](`Encoding::prefix`) and a non-empty [suffix](`Encoding::suffix`).
    pub const EMPTY: EncodingPrefix = 0;
    /// A stream of bytes.
    pub const APP_OCTET_STREAM: EncodingPrefix = 1;
    /// An application-specific encoding. Usually a [suffix](`Encoding::suffix`) is provided in the [`Encoding`].
    pub const APP_CUSTOM: EncodingPrefix = 2;
    /// An UTF-8 encoded string.
    pub const TEXT_PLAIN: EncodingPrefix = 3;
    /// A list of [`Properties`].
    pub const APP_PROPERTIES: EncodingPrefix = 4;
    /// A JSON intended to be consumed by an application.
    pub const APP_JSON: EncodingPrefix = 5;
    /// An SQL query.
    pub const APP_SQL: EncodingPrefix = 6;
    /// An signed or unsigned integer. Either 8bit, 16bit, 32bit, or 64bit.
    pub const APP_INTEGER: EncodingPrefix = 7;
    /// A 32bit or 64bit float.
    pub const APP_FLOAT: EncodingPrefix = 8;
    /// An XML document not readable from casual users ([RFC 3023, sec 3](https://www.rfc-editor.org/rfc/rfc3023#section-3)).
    pub const APP_XML: EncodingPrefix = 9;
    /// An XML document that likely has a root element from the HTML namespace
    pub const APP_XHTML_XML: EncodingPrefix = 10;
    /// A x-www-form-urlencoded list of tuples, each consisting of a name and a value.
    pub const APP_XWWW_FORM_URLENCODED: EncodingPrefix = 11;
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: EncodingPrefix = 12;
    /// An HTML document.
    pub const TEXT_HTML: EncodingPrefix = 13;
    /// An XML document readable from casual users ([RFC 3023, sec 3](https://www.rfc-editor.org/rfc/rfc3023#section-3)).
    pub const TEXT_XML: EncodingPrefix = 14;
    /// A CSS document.
    pub const TEXT_CSS: EncodingPrefix = 15;
    /// A CSV document.
    pub const TEXT_CSV: EncodingPrefix = 16;
    /// A Javascript program.
    pub const TEXT_JAVASCRIPT: EncodingPrefix = 17;
    /// A Jpeg image.
    pub const IMAGE_JPEG: EncodingPrefix = 18;
    /// A PNG image.
    pub const IMAGE_PNG: EncodingPrefix = 19;
    /// A GIF image.
    pub const IMAGE_GIF: EncodingPrefix = 20;

    /// A perfect hashmap for fast lookup of [`EncodingPrefix`] to string represenation.
    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        0u16 =>  "",
        1u16 =>  "application/octet-stream",
        2u16 =>  "application/custom",
        3u16 =>  "text/plain",
        4u16 =>  "application/properties",
        5u16 =>  "application/json",
        6u16 =>  "application/sql",
        7u16 =>  "application/integer",
        8u16 =>  "application/float",
        9u16 =>  "application/xml",
        10u16 => "application/xhtml+xml",
        11u16 => "application/x-www-form-urlencoded",
        12u16 => "text/json",
        13u16 => "text/html",
        14u16 => "text/xml",
        15u16 => "text/css",
        16u16 => "text/csv",
        17u16 => "text/javascript",
        18u16 => "image/jpeg",
        19u16 => "image/png",
        20u16 => "image/gif",
    };

    // A perfect hashmap for fast lookup of prefixes
    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "" => 0u16,
        "application/octet-stream" => 1u16,
        "application/custom" => 2u16,
        "text/plain" => 3u16,
        "application/properties" => 4u16,
        "application/json" => 5u16,
        "application/sql" => 6u16,
        "application/integer" => 7u16,
        "application/float" => 8u16,
        "application/xml" => 9u16,
        "application/xhtml+xml" => 10u16,
        "application/x-www-form-urlencoded" => 11u16,
        "text/json" => 12u16,
        "text/html" => 13u16,
        "text/xml" => 14u16,
        "text/css" => 15u16,
        "text/csv" => 16u16,
        "text/javascript" => 17u16,
        "image/jpeg" => 18u16,
        "image/png" => 19u16,
        "image/gif" => 20u16,
    };
}

impl EncodingMapping for DefaultEncodingMapping {
    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> &'static str {
        match Self::KNOWN_PREFIX.get(&p) {
            Some(p) => p,
            None => "unknown",
        }
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`DefaultEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> EncodingPrefix {
        match Self::KNOWN_STRING.get(s) {
            Some(p) => *p,
            None => Self::EMPTY,
        }
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`DefaultEncodingMapping::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse(_self: &DefaultEncodingMapping, t: Cow<'static, str>) -> ZResult<Encoding> {
            // Check if empty
            if t.is_empty() {
                return Ok(DefaultEncoding::EMPTY);
            }
            // Try first an exact lookup of the string to prefix
            let p = _self.str_to_prefix(t.as_ref());
            if p != DefaultEncodingMapping::EMPTY {
                return Ok(Encoding::new(p));
            }
            // Check if the passed string matches one of the known prefixes. It will map the known string
            // prefix to the numerical prefix and carry the remaining part of the string in the suffix.
            // Skip empty string mapping. The order is guaranteed by the phf::OrderedMap.
            for (s, p) in DefaultEncodingMapping::KNOWN_STRING.entries().skip(1) {
                if let Some(i) = t.find(s) {
                    let e = Encoding::new(*p);
                    match t {
                        Cow::Borrowed(s) => return e.with_suffix(s.split_at(i + s.len()).1),
                        Cow::Owned(mut s) => return e.with_suffix(s.split_off(i + s.len())),
                    }
                }
            }
            // No matching known prefix has been found, carry everything in the suffix.
            DefaultEncoding::EMPTY.with_suffix(t)
        }
        _parse(self, t.into())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str<'a>(&self, e: &'a Encoding) -> Cow<'a, str> {
        if e.prefix() == DefaultEncodingMapping::EMPTY {
            Cow::Borrowed(e.suffix())
        } else {
            Cow::Owned(format!("{}{}", self.prefix_to_str(e.prefix()), e.suffix()))
        }
    }
}

/// Default [`Encoding`] provided with Zenoh to facilitate the encoding and decoding
/// of [`Value`]s in the Rust API. Please note that Zenoh does not impose any
/// encoding and users are free to use any encoder they like. [`DefaultEncoding`]
/// is simply provided as convenience to the users.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefaultEncoding;

impl DefaultEncoding {
    /// Encoding is unspecified. Applications are expected to decode messages
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
