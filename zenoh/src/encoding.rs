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
use phf::phf_map;
use std::{borrow::Cow, convert::Infallible, fmt, str::FromStr};
use zenoh_buffers::ZSlice;
use zenoh_protocol::core::EncodingId;

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
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding(zenoh_protocol::core::Encoding);

impl Encoding {
    const SEPARATOR: char = ';';

    // - Primitives types supported in all Zenoh bindings
    /// See [`DefaultEncodingMapping::ZENOH_BYTES`].
    pub const ZENOH_BYTES: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 0,
        schema: None,
    });
    /// A VLE-encoded signed little-endian integer. Either 8bit, 16bit, 32bit, or 64bit.
    /// Binary reprensentation uses two's complement.
    pub const ZENOH_INT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 1,
        schema: None,
    });
    /// A VLE-encoded little-endian unsigned integer. Either 8bit, 16bit, 32bit, or 64bit.
    pub const ZENOH_UINT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 2,
        schema: None,
    });
    /// A VLE-encoded float. Either little-endian 32bit or 64bit.
    /// Binary representation uses *IEEE 754-2008* *binary32* or *binary64*, respectively.
    pub const ZENOH_FLOAT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 3,
        schema: None,
    });
    /// A boolean. `0` is `false`, `1` is `true`. Other values are invalid.
    pub const ZENOH_BOOL: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 4,
        schema: None,
    });
    /// A UTF-8 string.
    pub const ZENOH_STRING: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 5,
        schema: None,
    });
    /// A zenoh error.
    pub const ZENOH_ERRROR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 6,
        schema: None,
    });

    // - Advanced types. The may be supported in some Zenoh bindings.
    /// An application-specific stream of bytes.
    pub const APPLICATION_OCTET_STREAM: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 7,
        schema: None,
    });
    /// A textual file.
    pub const TEXT_PLAIN: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 8,
        schema: None,
    });
    /// A JSON intended to be consumed by an application.
    pub const APPLICATION_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 9,
        schema: None,
    });
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 10,
        schema: None,
    });
    /// A Common Data Representation (CDR)-encoded data. A [suffix](`Encoding::suffix`) may be provided in the [`Encoding`] to specify the concrete type.
    pub const APPLICATION_CDR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 11,
        schema: None,
    });

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

    pub const fn default() -> Self {
        Self::ZENOH_BYTES
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::default()
    }
}

impl From<&str> for Encoding {
    fn from(t: &str) -> Self {
        let mut inner = zenoh_protocol::core::Encoding::empty();

        // Check if empty
        if !t.is_empty() {
            return Encoding(inner);
        }

        // Everything before `;` may be mapped to a known id
        let (id, schema) = t.split_once(Encoding::SEPARATOR).unwrap_or((t, ""));
        if let Some(id) = Encoding::STR_TO_ID.get(id).copied() {
            inner.id = id;
        };
        if !schema.is_empty() {
            inner.schema = Some(ZSlice::from(schema.to_string().into_bytes()));
        }

        Encoding(inner)
    }
}

impl From<String> for Encoding {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl FromStr for Encoding {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl From<&Encoding> for Cow<'static, str> {
    fn from(encoding: &Encoding) -> Self {
        fn su8_to_str(schema: &[u8]) -> &str {
            std::str::from_utf8(schema).unwrap_or("unknown(non-utf8)")
        }

        match (
            Encoding::ID_TO_STR.get(&encoding.0.id).copied(),
            encoding.0.schema.as_ref(),
        ) {
            // Perfect match
            (Some(i), None) => Cow::Borrowed(i),
            // ID and schema
            (Some(i), Some(s)) => {
                Cow::Owned(format!("{}{}{}", i, Encoding::SEPARATOR, su8_to_str(s)))
            }
            //
            (None, Some(s)) => Cow::Owned(format!(
                "unknown({}){}{}",
                encoding.0.id,
                Encoding::SEPARATOR,
                su8_to_str(s)
            )),
            (None, None) => Cow::Owned(format!("unknown({})", encoding.0.id)),
        }
    }
}

impl From<Encoding> for Cow<'static, str> {
    fn from(encoding: Encoding) -> Self {
        Self::from(&encoding)
    }
}

impl From<Encoding> for String {
    fn from(encoding: Encoding) -> Self {
        encoding.to_string()
    }
}

impl From<Encoding> for zenoh_protocol::core::Encoding {
    fn from(value: Encoding) -> Self {
        value.0
    }
}

impl From<zenoh_protocol::core::Encoding> for Encoding {
    fn from(value: zenoh_protocol::core::Encoding) -> Self {
        Self(value)
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let s = Cow::from(self);
        f.write_str(s.as_ref())
    }
}
