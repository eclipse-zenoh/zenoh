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
use super::ExtendedEncodingMapping;
use phf::phf_ordered_map;
use std::borrow::Cow;
use zenoh::encoding::{Encoding, EncodingMapping, EncodingPrefix};
use zenoh_result::ZResult;

/// Encoding mapping used by the [`SerdeEncoding`]. It has been generated starting from the
/// MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml).
/// [`SerdeEncoding`] covers the types defined in the `2024-02-16` IANA update.
#[derive(Clone, Copy, Debug)]
pub struct SerdeEncoding;

// Extended Encoding
pub type ExtendedSerdeEncoding = ExtendedEncodingMapping<SerdeEncoding>;

// Start from 1024 - non-reserved prefixes.
impl SerdeEncoding {
    pub const APPLICATION_JSON: EncodingPrefix = 0;
    pub const APPLICATION_CBOR: EncodingPrefix = 1025;
    pub const SERDE: EncodingPrefix = 65_535;

    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        1024u16 => "application/cbor",
        65535u16 => "serde/",

    };

    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "application/cbor" => 1024u16,
        "serde/" => 65535u16,
    };
}

impl EncodingMapping for SerdeEncoding {
    const MIN: EncodingPrefix = 1024;
    const MAX: EncodingPrefix = EncodingPrefix::MAX;

    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> Option<Cow<'static, str>> {
        Self::KNOWN_PREFIX.get(&p).map(|s| Cow::Borrowed(*s))
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`IanaEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> Option<EncodingPrefix> {
        Self::KNOWN_STRING.get(s).copied()
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`SerdeEncoding::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse(t: Cow<'static, str>) -> ZResult<Encoding> {
            if t.is_empty() {
                return Ok(Encoding::empty());
            }

            // Skip empty string mapping. The order is guaranteed by the phf::OrderedMap.
            for (s, p) in SerdeEncoding::KNOWN_STRING.entries().skip(1) {
                if let Some(i) = t.find(s) {
                    let e = Encoding::new(*p);
                    match t {
                        Cow::Borrowed(s) => return e.with_suffix(s.split_at(i + s.len()).1),
                        Cow::Owned(mut s) => return e.with_suffix(s.split_off(i + s.len())),
                    }
                }
            }
            Encoding::empty().with_suffix(t)
        }
        _parse(t.into())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str(&self, e: &Encoding) -> Cow<'_, str> {
        let (p, s) = (e.prefix(), e.suffix());
        match self.prefix_to_str(p) {
            Some(p) if s.is_empty() => p,
            Some(p) => Cow::Owned(format!("{}{}", p, s)),
            None => Cow::Owned(format!("unknown({}){}", p, s)),
        }
    }
}

// crate::derive_default_encoding_for!(SerdeEncoding);
// pub struct SerdeCbor;

// impl<T> Encoder<T> for SerdeEncoding
// where
//     T: Serialize,
// {
//     type Output = Result<Value, serde_json::Error>;

//     fn encode(self, t: T) -> Self::Output {
//         let s = serde_cbor::to_string(&t)?;

//         let v =
//             Value::new(ZBuf::from(s.into_bytes())).with_encoding(DefaultEncoding::APPLICATION_JSON);
//         Ok(v)
//     }
// }

// pub struct SerdeJson;

// impl<T> Encoder<T> for SerdeJson
// where
//     T: Serialize,
// {
//     type Output = Result<Value, serde_json::Error>;

//     fn encode(self, t: T) -> Self::Output {
//         let s = serde_json::to_string(&t)?;
//         let v =
//             Value::new(ZBuf::from(s.into_bytes())).with_encoding(DefaultEncoding::APPLICATION_JSON);
//         Ok(v)
//     }
// }

// impl<T> Decoder<T> for SerdeJson
// where
//     T: Serialize,
// {
//     type Error = serde_json::Error;

//     fn decode(self, t: &Value) -> Result<T, Self::Error> {
//         if t.encoding == DefaultEncoding::APPLICATION_JSON {
//             let s: String = String::from_utf8(v.payload.contiguous().to_vec())
//                 .map_err(|e| zerror!("{}", e))
//                 .unwrap();
//             serde_json::from_value(value)
//         } else {
//         }
//     }
// }
