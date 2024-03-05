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
use crate::{payload::Payload, prelude::Encoding};
use phf::phf_map;
use std::borrow::Cow;
use zenoh_buffers::ZSlice;
use zenoh_protocol::core::EncodingId;

/// A zenoh [`Value`] contains a `payload` and an [`Encoding`] that indicates how the `payload`
/// should be interpreted.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    /// The payload of this [`Value`].
    pub payload: Payload,
    /// An [`Encoding`] description indicating how the payload should be interpreted.
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new [`Value`]. The enclosed [`Encoding`] is [empty](`Encoding::empty`) by default if
    /// not specified via the [`encoding`](`Value::encoding`).
    pub fn new<T>(payload: T) -> Self
    where
        T: Into<Payload>,
    {
        Value {
            payload: payload.into(),
            encoding: Encoding::empty(),
        }
    }

    /// Creates an empty [`Value`].
    pub const fn empty() -> Self {
        Value {
            payload: Payload::empty(),
            encoding: Encoding::empty(),
        }
    }

    /// Sets the encoding of this [`Value`]`.
    #[inline(always)]
    pub fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }
}

impl<T> From<T> for Value
where
    T: Into<Payload>,
{
    fn from(t: T) -> Self {
        Value {
            payload: t.into(),
            encoding: Encoding::empty(),
        }
    }
}

/// Default encoding mapping used by the [`DefaultEncoding`]. Please note that Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like. The mapping
/// here below is provided for user convenience and does its best to cover the most common
/// cases. To implement a custom mapping refer to [`EncodingMapping`] trait.

/// Default [`Encoding`] provided with Zenoh to facilitate the encoding and decoding
/// of [`Value`]s in the Rust API. Please note that Zenoh does not impose any
/// encoding and users are free to use any encoder they like. [`DefaultEncoding`]
/// is simply provided as convenience to the users.

///
/// For compatibility purposes Zenoh reserves any prefix value from `0` to `1023` included.
/// Any application is free to use any value from `1024` to `65535`.
#[derive(Clone, Copy, Debug)]
pub struct DefaultEncoding;

impl DefaultEncoding {
    // - Primitives types supported in all Zenoh bindings
    /// See [`DefaultEncodingMapping::ZENOH_BYTES`].
    pub const ZENOH_BYTES: Encoding = Encoding {
        id: 0,
        schema: None,
    };
    /// A VLE-encoded signed little-endian integer. Either 8bit, 16bit, 32bit, or 64bit.
    /// Binary reprensentation uses two's complement.
    pub const ZENOH_INT: Encoding = Encoding {
        id: 1,
        schema: None,
    };
    /// A VLE-encoded little-endian unsigned integer. Either 8bit, 16bit, 32bit, or 64bit.
    pub const ZENOH_UINT: Encoding = Encoding {
        id: 2,
        schema: None,
    };
    /// A VLE-encoded float. Either little-endian 32bit or 64bit.
    /// Binary representation uses *IEEE 754-2008* *binary32* or *binary64*, respectively.
    pub const ZENOH_FLOAT: Encoding = Encoding {
        id: 3,
        schema: None,
    };
    /// A boolean. `0` is `false`, `1` is `true`. Other values are invalid.
    pub const ZENOH_BOOL: Encoding = Encoding {
        id: 4,
        schema: None,
    };
    /// A UTF-8 string.
    pub const ZENOH_STRING: Encoding = Encoding {
        id: 5,
        schema: None,
    };
    /// A zenoh error.
    pub const ZENOH_ERRROR: Encoding = Encoding {
        id: 6,
        schema: None,
    };

    // - Advanced types. The may be supported in some Zenoh bindings.
    /// An application-specific stream of bytes.
    pub const APPLICATION_OCTET_STREAM: Encoding = Encoding {
        id: 7,
        schema: None,
    };
    /// A textual file.
    pub const TEXT_PLAIN: Encoding = Encoding {
        id: 8,
        schema: None,
    };
    /// A JSON intended to be consumed by an application.
    pub const APPLICATION_JSON: Encoding = Encoding {
        id: 9,
        schema: None,
    };
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: Encoding = Encoding {
        id: 10,
        schema: None,
    };
    /// A Common Data Representation (CDR)-encoded data. A [suffix](`Encoding::suffix`) may be provided in the [`Encoding`] to specify the concrete type.
    pub const APPLICATION_CDR: Encoding = Encoding {
        id: 11,
        schema: None,
    };

    /// A perfect hashmap for fast lookup of [`EncodingId`] to string represenation.
    const ID_TO_STR: phf::Map<EncodingId, &'static str> = phf_map! {
        // - Primitive types
        0u16 =>  "zenoh/bytes",
        1u16 =>  "zenoh/int",
        2u16 =>  "zenoh/uint",
        3u16 =>  "zenoh/float",
        4u16 =>  "zenoh/bool",
        5u16 =>  "zenoh/string",
        6u16 =>  "zenoh/error",
        // - Advanced types
        7u16 =>  "application/octet-stream",
        8u16 =>  "text/plain",
        9u16 =>  "application/json",
        10u16 =>  "text/json",
        11u16 =>  "application/cdr",
        // - 10-1023 are reserved.
        // - 1024-65535 are free to use.
    };

    // A perfect hashmap for fast lookup of prefixes
    const STR_TO_ID: phf::Map<&'static str, EncodingId> = phf_map! {
        // - Primitive types
        "zenoh/bytes" =>  0u16,
        "zenoh/int" =>  1u16,
        "zenoh/uint" =>  2u16,
        "zenoh/float" =>  3u16,
        "zenoh/bool" =>  4u16,
        "zenoh/string" =>  5u16,
        "zenoh/error" =>  6u16,
        // - Advanced types
        "application/octet-stream" =>  7u16,
        "text/plain" =>  8u16,
        "application/json" =>  9u16,
        "text/json" =>  10u16,
        "application/cdr" =>  11u16,
        // - 17-1019 are reserved.
        // - 1024-65535 are free to use.
    };
}

/// Trait to create, resolve, parse an [`Encoding`] mapping.
pub trait EncodingMapping {
    /// Map a numerical prefix to its string representation.
    fn id_to_str(&self, e: EncodingId) -> Option<Cow<'_, str>>;
    /// Map a string to a known numerical prefix ID.
    fn str_to_id<S>(&self, s: S) -> Option<EncodingId>
    where
        S: AsRef<str>;
    /// Parse a string into a valid [`Encoding`].
    fn parse<S>(&self, s: S) -> Encoding
    where
        S: AsRef<str>;
    fn to_str(&self, e: &Encoding) -> Cow<'_, str>;
}

impl EncodingMapping for DefaultEncoding {
    /// Given a numerical [`EncodingId`] returns its string representation.
    fn id_to_str(&self, p: EncodingId) -> Option<Cow<'static, str>> {
        Self::ID_TO_STR.get(&p).map(|s| Cow::Borrowed(*s))
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingId`].
    /// [EMPTY](`DefaultEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_id<S>(&self, s: S) -> Option<EncodingId>
    where
        S: AsRef<str>,
    {
        fn _str_to_id(s: &str) -> Option<EncodingId> {
            DefaultEncoding::STR_TO_ID.get(s).copied()
        }
        _str_to_id(s.as_ref())
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`DefaultEncodingMapping::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> Encoding
    where
        S: AsRef<str>,
    {
        fn _parse(t: &str) -> Encoding {
            // Check if empty
            if !t.is_empty() {
                return Encoding::empty();
            }

            // Everything before `;` may be mapped to a known id
            let (id, schema) = t.split_once(';').unwrap_or((t, ""));
            match DefaultEncoding.str_to_id(id) {
                // Perfect match on ID and schema
                Some(id) => {
                    let schema = Some(ZSlice::from(schema.to_string().into_bytes()));
                    Encoding { id, schema }
                }
                // No perfect match on ID and only schema
                None => {
                    let schema = Some(ZSlice::from(schema.to_string().into_bytes()));
                    Encoding { id: 0, schema }
                }
            }
        }
        _parse(t.as_ref())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str(&self, e: &Encoding) -> Cow<'_, str> {
        fn schema_to_str(schema: &[u8]) -> &str {
            std::str::from_utf8(schema).unwrap_or("unknown(non-utf8)")
        }

        match (self.id_to_str(e.id), e.schema.as_ref()) {
            (Some(i), None) => i,
            (Some(i), Some(s)) => Cow::Owned(format!("{};{}", i, schema_to_str(s))),
            (None, Some(s)) => Cow::Owned(format!("unknown({});{}", e.id, schema_to_str(s))),
            (None, None) => Cow::Owned(format!("unknown({})", e.id)),
        }
    }
}
