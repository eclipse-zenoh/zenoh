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
use std::{borrow::Cow, sync::Arc};
use zenoh_buffers::{buffer::SplitBuffer, ZBuf, ZSlice};
use zenoh_protocol::core::{Encoding, EncodingPrefix};
use zenoh_result::ZResult;
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryBuf;

/// Default encoding mapping used by the [`DefaultEncoding`]. Please note that Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like. The mapping
/// here below is provided for user convenience and does its best to cover the most common
/// cases. To implement a custom mapping refer to [`EncodingMapping`] trait.
///
/// For compatibility purposes Zenoh reserves any prefix value from `0` to `1023` included.
/// Any application is free to use any value from `1024` to `65535`.
#[derive(Clone, Copy, Debug)]
pub struct DefaultEncodingMapping;

impl DefaultEncodingMapping {
    /// Unspecified [`EncodingPrefix`].
    /// Note that an [`Encoding`] could have an empty [prefix](`Encoding::prefix`) and a non-empty [suffix](`Encoding::suffix`).
    pub const EMPTY: EncodingPrefix = 0;

    // - Primitives types supported in all Zenoh bindings
    /// A stream of bytes.
    pub const APPLICATION_OCTET_STREAM: EncodingPrefix = 1;
    /// A signed integer.
    pub const ZENOH_INT: EncodingPrefix = 2;
    /// An unsigned integer.
    pub const ZENOH_UINT: EncodingPrefix = 3;
    /// A float.
    pub const ZENOH_FLOAT: EncodingPrefix = 4;
    /// A boolean. `0` is `false`, `1` is `true`. Other values are invalid.
    pub const ZENOH_BOOL: EncodingPrefix = 5;
    /// A UTF-8 encoded string.
    pub const TEXT_PLAIN: EncodingPrefix = 6;

    // - Advanced types supported in some Zenoh bindings.
    /// A JSON intended to be consumed by an application.
    pub const APPLICATION_JSON: EncodingPrefix = 7;
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: EncodingPrefix = 8;

    // - 9-15 are reserved

    // - A list of common prefixes
    /// An application-specific encoding. Usually a [suffix](`Encoding::suffix`) is provided in the [`Encoding`].
    pub const APPLICATION_CUSTOM: EncodingPrefix = 16;
    /// A Common Data Representation (CDR)-encoded data. A [suffix](`Encoding::suffix`) may be provided in the [`Encoding`] to specify the concrete type.
    pub const APPLICATION_CDR: EncodingPrefix = 17;

    // - 18-58 are reserved

    /// Common prefix for Zenoh-defined types.
    pub const ZENOH: EncodingPrefix = 58;

    // - A list of most common registries from IANA.
    // - The highest prefix value to fit in 1 byte on the wire is 63.
    /// Common prefix for *application* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#application).
    pub const APPLICATION: EncodingPrefix = 59;
    /// Common prefix for *audio* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#audio).
    pub const AUDIO: EncodingPrefix = 60;
    /// Common prefix for *image* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#image).
    pub const IMAGE: EncodingPrefix = 61;
    /// Common prefix for *text* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#text).
    pub const TEXT: EncodingPrefix = 62;
    /// Common prefix for *video* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#video).
    pub const VIDEO: EncodingPrefix = 63;

    // - 64-1019 are reserved.

    // - A list of least common registries from IANA.
    /// Common prefix for *font* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#font).
    pub const FONT: EncodingPrefix = 1_020;
    /// Common prefix for *message* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#message).
    pub const MESSAGE: EncodingPrefix = 1_021;
    /// Common prefix for *model* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#model).
    pub const MODEL: EncodingPrefix = 1_022;
    /// Common prefix for *multipart* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#multipart).
    pub const MULTIPART: EncodingPrefix = 1_023;

    // - 1024-65535 are free to use.

    /// A perfect hashmap for fast lookup of [`EncodingPrefix`] to string represenation.
    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        0u16 =>  "",
        // - Primitive types
        1u16 =>  "application/octet-stream",
        2u16 =>  "zenoh/int",
        3u16 =>  "zenoh/uint",
        4u16 =>  "zenoh/float",
        5u16 =>  "zenoh/bool",
        6u16 =>  "text/plain",
        // - Advanced types
        7u16 =>  "application/json",
        8u16 =>  "text/json",
        // - 9-15 are reserved.
        16u16 =>  "application/custom",
        17u16 =>  "application/cdr",
        // - 18-58 are reserved.
        58u16 => "zenoh/",
        59u16 => "application/",
        60u16 => "audio/",
        61u16 => "image/",
        62u16 => "text/",
        63u16 => "video/",
        // - 64-1019 are reserved.
        1_020u16 => "font/",
        1_021u16 => "message/",
        1_022u16 => "model/",
        1_023u16 => "multipart/",
        // - 1024-65535 are free to use.
    };

    // A perfect hashmap for fast lookup of prefixes
    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "" =>  0u16,
        // - Primitive types
        "application/octet-stream" =>  1u16,
        "zenoh/int" =>  2u16,
        "zenoh/uint" =>  3u16,
        "zenoh/float" =>  4u16,
        "zenoh/bool" =>  5u16,
        "text/plain" =>  6u16,
        // - Advanced types
        "application/json" =>  7u16,
        "text/json" =>  8u16,
        // - 9-15 are reserved.
        "application/custom" =>  16u16,
        "application/cdr" =>  17u16,
        // - 18-58 are reserved.
        "zenoh/" => 58u16,
        "application/" => 59u16,
        "audio/" => 60u16,
        "image/" => 61u16,
        "text/" => 62u16,
        "video/" => 63u16,
        // - 64-1019 are reserved.
        "font/" => 1_020u16,
        "message/" => 1_021u16,
        "model/" => 1_022u16,
        "multipart/" => 1_023u16,
        // - 1024-65535 are free to use.
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
    /// See [`DefaultEncodingMapping::EMPTY`].
    pub const EMPTY: Encoding = Encoding::new(DefaultEncodingMapping::EMPTY);
    /// An application-specific stream of bytes.
    pub const APPLICATION_OCTET_STREAM: Encoding =
        Encoding::new(DefaultEncodingMapping::APPLICATION_OCTET_STREAM);
    /// A VLE-encoded signed little-endian integer. Either 8bit, 16bit, 32bit, or 64bit.
    /// Binary reprensentation uses two's complement.
    pub const ZENOH_INT: Encoding = Encoding::new(DefaultEncodingMapping::ZENOH_INT);
    /// A VLE-encoded little-endian unsigned integer. Either 8bit, 16bit, 32bit, or 64bit.
    pub const ZENOH_UINT: Encoding = Encoding::new(DefaultEncodingMapping::ZENOH_UINT);
    /// See [`DefaultEncodingMapping::ZENOH_FLOAT`].
    pub const ZENOH_FLOAT: Encoding = Encoding::new(DefaultEncodingMapping::ZENOH_FLOAT);
    /// See [`DefaultEncodingMapping::ZENOH_BOOL`].
    pub const ZENOH_BOOL: Encoding = Encoding::new(DefaultEncodingMapping::ZENOH_BOOL);
    pub const TEXT_PLAIN: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_PLAIN);
    pub const APPLICATION_JSON: Encoding = Encoding::new(DefaultEncodingMapping::APPLICATION_JSON);
    pub const TEXT_JSON: Encoding = Encoding::new(DefaultEncodingMapping::TEXT_JSON);

    pub const APPLICATION_CUSTOM: Encoding =
        Encoding::new(DefaultEncodingMapping::APPLICATION_CUSTOM);
    pub const APPLICATION_CDR: Encoding = Encoding::new(DefaultEncodingMapping::APPLICATION_CDR);
}

// - Zenoh primitive types encoders/decoders
// Octect stream
impl Encoder<ZBuf> for DefaultEncoding {
    fn encode(self, t: ZBuf) -> Value {
        Value {
            payload: t,
            encoding: DefaultEncoding::APPLICATION_OCTET_STREAM,
        }
    }
}

impl Decoder<ZBuf> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<ZBuf> {
        if v.encoding == DefaultEncoding::APPLICATION_OCTET_STREAM {
            Ok(v.payload.clone())
        } else {
            Err(zerror!(
                "Decode error: invalid encoding. Received '{}', expected '{}'.",
                DefaultEncodingMapping.to_str(&v.encoding),
                DefaultEncodingMapping.to_str(&DefaultEncoding::APPLICATION_OCTET_STREAM)
            )
            .into())
        }
    }
}

impl Encoder<Vec<u8>> for DefaultEncoding {
    fn encode(self, t: Vec<u8>) -> Value {
        Self.encode(ZBuf::from(t))
    }
}

impl Encoder<&[u8]> for DefaultEncoding {
    fn encode(self, t: &[u8]) -> Value {
        Self.encode(t.to_vec())
    }
}

impl Decoder<Vec<u8>> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<Vec<u8>> {
        let v: ZBuf = Self.decode(v)?;
        Ok(v.contiguous().to_vec())
    }
}

impl<'a> Encoder<Cow<'a, [u8]>> for DefaultEncoding {
    fn encode(self, t: Cow<'a, [u8]>) -> Value {
        Self.encode(ZBuf::from(t.to_vec()))
    }
}

impl<'a> Decoder<Cow<'a, [u8]>> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<Cow<'a, [u8]>> {
        let v: Vec<u8> = Self.decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// Text plain
impl Encoder<String> for DefaultEncoding {
    fn encode(self, s: String) -> Value {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: DefaultEncoding::TEXT_PLAIN,
        }
    }
}

impl Encoder<&str> for DefaultEncoding {
    fn encode(self, s: &str) -> Value {
        Self.encode(s.to_string())
    }
}

impl Decoder<String> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<String> {
        if v.encoding == DefaultEncoding::TEXT_PLAIN {
            String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e).into())
        } else {
            Err(zerror!(
                "Decode error: invalid encoding. Received '{}', expected '{}'.",
                DefaultEncodingMapping.to_str(&v.encoding),
                DefaultEncodingMapping.to_str(&DefaultEncoding::TEXT_PLAIN)
            )
            .into())
        }
    }
}

impl<'a> Encoder<Cow<'a, str>> for DefaultEncoding {
    fn encode(self, s: Cow<'a, str>) -> Value {
        Self.encode(s.to_string())
    }
}

impl<'a> Decoder<Cow<'a, str>> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<Cow<'a, str>> {
        let v: String = Self.decode(v)?;
        Ok(Cow::Owned(v))
    }
}

// - Integers impl
macro_rules! impl_int {
    ($t:ty, $encoding:expr) => {
        impl Encoder<$t> for DefaultEncoding {
            fn encode(self, t: $t) -> Value {
                let bs = t.to_le_bytes();
                let end = 1 + bs.iter().rposition(|b| *b != 0).unwrap_or(bs.len() - 1);
                let v = Value {
                    // Safety:
                    // - 0 is a valid start index because bs is guaranteed to always have a length greater or equal than 1
                    // - end is a valid end index because is bounded between 0 and bs.len()
                    payload: ZBuf::from(unsafe { ZSlice::new_unchecked(Arc::new(bs), 0, end) }),
                    encoding: $encoding,
                };
                v
            }
        }

        impl Encoder<&$t> for DefaultEncoding {
            fn encode(self, t: &$t) -> Value {
                Self.encode(*t)
            }
        }

        impl Encoder<&mut $t> for DefaultEncoding {
            fn encode(self, t: &mut $t) -> Value {
                Self.encode(*t)
            }
        }

        impl Decoder<$t> for DefaultEncoding {
            fn decode(self, v: &Value) -> ZResult<$t> {
                if v.encoding == $encoding {
                    let p = v.payload.contiguous();
                    let mut bs = (0 as $t).to_le_bytes();
                    if p.len() > bs.len() {
                        return Err(zerror!(
                            "Decode error: {} invalid length ({} > {})",
                            std::any::type_name::<$t>(),
                            p.len(),
                            bs.len()
                        )
                        .into());
                    }
                    bs[..p.len()].copy_from_slice(&p);
                    let t = <$t>::from_le_bytes(bs);
                    Ok(t)
                } else {
                    Err(zerror!(
                        "Decode error: invalid encoding. Received '{}', expected '{}'.",
                        DefaultEncodingMapping.to_str(&v.encoding),
                        DefaultEncodingMapping.to_str(&$encoding)
                    )
                    .into())
                }
            }
        }
    };
}

// Zenoh unsigned integers
impl_int!(u8, DefaultEncoding::ZENOH_UINT);
impl_int!(u16, DefaultEncoding::ZENOH_UINT);
impl_int!(u32, DefaultEncoding::ZENOH_UINT);
impl_int!(u64, DefaultEncoding::ZENOH_UINT);
impl_int!(usize, DefaultEncoding::ZENOH_UINT);

// Zenoh signed integers
impl_int!(i8, DefaultEncoding::ZENOH_INT);
impl_int!(i16, DefaultEncoding::ZENOH_INT);
impl_int!(i32, DefaultEncoding::ZENOH_INT);
impl_int!(i64, DefaultEncoding::ZENOH_INT);
impl_int!(isize, DefaultEncoding::ZENOH_INT);

// Zenoh floats
impl_int!(f32, DefaultEncoding::ZENOH_FLOAT);
impl_int!(f64, DefaultEncoding::ZENOH_FLOAT);

// Zenoh bool
impl Encoder<bool> for DefaultEncoding {
    fn encode(self, t: bool) -> Value {
        // SAFETY: casting a bool into an integer is well-defined behaviour.
        //      0 is false, 1 is true: https://doc.rust-lang.org/std/primitive.bool.html
        Value {
            payload: ZBuf::from((t as u8).to_le_bytes()),
            encoding: DefaultEncoding::APPLICATION_JSON,
        }
    }
}

impl Decoder<bool> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<bool> {
        if v.encoding == DefaultEncoding::ZENOH_BOOL {
            let p = v.payload.contiguous();
            if p.len() != 1 {
                return Err(
                    zerror!("Decode error:: bool invalid length ({} != {})", p.len(), 1).into(),
                );
            }
            match p[0] {
                0 => Ok(false),
                1 => Ok(true),
                invalid => Err(zerror!("Decode error: bool invalid value ({})", invalid).into()),
            }
        } else {
            Err(zerror!(
                "Decode error: invalid encoding. Received '{}', expected '{}'.",
                DefaultEncodingMapping.to_str(&v.encoding),
                DefaultEncodingMapping.to_str(&DefaultEncoding::ZENOH_BOOL)
            )
            .into())
        }
    }
}

// - Zenoh advanced types encoders/decoders
// JSON
impl Encoder<&serde_json::Value> for DefaultEncoding {
    fn encode(self, t: &serde_json::Value) -> Value {
        Value {
            payload: ZBuf::from(t.to_string().into_bytes()),
            encoding: DefaultEncoding::APPLICATION_JSON,
        }
    }
}

impl Encoder<serde_json::Value> for DefaultEncoding {
    fn encode(self, t: serde_json::Value) -> Value {
        Self.encode(&t)
    }
}

impl Decoder<serde_json::Value> for DefaultEncoding {
    fn decode(self, v: &Value) -> ZResult<serde_json::Value> {
        if v.encoding == DefaultEncoding::APPLICATION_JSON
            || v.encoding == DefaultEncoding::TEXT_JSON
        {
            let r: serde_json::Value = serde::Deserialize::deserialize(
                &mut serde_json::Deserializer::from_slice(&v.payload.contiguous()),
            )
            .map_err(|e| zerror!("Decode error: {}", e))?;
            Ok(r)
        } else {
            Err(zerror!(
                "Decode error: invalid encoding. Received '{}', expected '{}' or '{}'.",
                DefaultEncodingMapping.to_str(&v.encoding),
                DefaultEncodingMapping.to_str(&DefaultEncoding::APPLICATION_JSON),
                DefaultEncodingMapping.to_str(&DefaultEncoding::TEXT_JSON)
            )
            .into())
        }
    }
}

// - Zenoh Rust-specific types encoders/decoders
// Sample
impl Encoder<Sample> for DefaultEncoding {
    fn encode(self, t: Sample) -> Value {
        t.value
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl Encoder<Arc<SharedMemoryBuf>> for DefaultEncoding {
    fn encode(self, t: Arc<SharedMemoryBuf>) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoding::APPLICATION_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<Box<SharedMemoryBuf>> for DefaultEncoding {
    fn encode(self, t: Box<SharedMemoryBuf>) -> Value {
        let smb: Arc<SharedMemoryBuf> = t.into();
        Self.encode(smb)
    }
}

#[cfg(feature = "shared-memory")]
impl Encoder<SharedMemoryBuf> for DefaultEncoding {
    fn encode(self, t: SharedMemoryBuf) -> Value {
        Value {
            payload: t.into(),
            encoding: DefaultEncoding::APPLICATION_OCTET_STREAM,
        }
    }
}

mod tests {
    #[test]
    fn encoder() {
        use crate::Value;
        use rand::Rng;
        use zenoh_buffers::ZBuf;

        const NUM: usize = 1_000;

        macro_rules! encode_decode {
            ($t:ty, $in:expr) => {
                let i = $in;
                let t = i.clone();
                let v = Value::encode(t);
                let o: $t = v.decode().unwrap();
                assert_eq!(i, o)
            };
        }

        let mut rng = rand::thread_rng();

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

        for _ in 0..NUM {
            encode_decode!(u8, rng.gen::<u8>());
            encode_decode!(u16, rng.gen::<u16>());
            encode_decode!(u32, rng.gen::<u32>());
            encode_decode!(u64, rng.gen::<u64>());
            encode_decode!(usize, rng.gen::<usize>());
        }

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

        for _ in 0..NUM {
            encode_decode!(i8, rng.gen::<i8>());
            encode_decode!(i16, rng.gen::<i16>());
            encode_decode!(i32, rng.gen::<i32>());
            encode_decode!(i64, rng.gen::<i64>());
            encode_decode!(isize, rng.gen::<isize>());
        }

        encode_decode!(f32, f32::MIN);
        encode_decode!(f64, f64::MIN);

        encode_decode!(f32, f32::MAX);
        encode_decode!(f64, f64::MAX);

        for _ in 0..NUM {
            encode_decode!(f32, rng.gen::<f32>());
            encode_decode!(f64, rng.gen::<f64>());
        }

        encode_decode!(String, "");
        encode_decode!(String, String::from("abcdefghijklmnopqrstuvwxyz"));

        encode_decode!(Vec<u8>, vec![0u8; 0]);
        encode_decode!(Vec<u8>, vec![0u8; 64]);

        encode_decode!(ZBuf, ZBuf::from(vec![0u8; 0]));
        encode_decode!(ZBuf, ZBuf::from(vec![0u8; 64]));
    }
}
