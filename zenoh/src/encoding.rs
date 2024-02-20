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
use std::{
    fmt,
    ops::{Deref, DerefMut},
    str::FromStr,
};
use zenoh_protocol::core::{Encoding as WireEncoding, EncodingPrefix};
use zenoh_result::ZResult;

pub trait EncodingMapping {
    fn prefix_to_str(&self, e: zenoh_protocol::core::EncodingPrefix) -> &str;
    fn str_to_prefix(&self, s: &str) -> zenoh_protocol::core::EncodingPrefix;
    fn parse(s: &str) -> ZResult<WireEncoding>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding<T = DefaultEncodingMapping>
where
    T: EncodingMapping,
{
    encoding: WireEncoding,
    mapping: T,
}

impl<T> fmt::Display for Encoding<T>
where
    T: EncodingMapping,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.mapping.prefix_to_str(self.encoding.prefix()))?;
        f.write_str(self.encoding.suffix())
    }
}

impl<T> Encoding<T>
where
    T: EncodingMapping,
{
    const fn exact(prefix: EncodingPrefix, mapping: T) -> Self {
        Self {
            encoding: WireEncoding::new(prefix),
            mapping,
        }
    }

    pub fn mapping(&self) -> &T {
        &self.mapping
    }

    pub fn mapping_mut(&mut self) -> &mut T {
        &mut self.mapping
    }

    pub fn with_mapping<U>(self, mapping: U) -> Encoding<U>
    where
        U: EncodingMapping,
    {
        let Self { encoding, .. } = self;
        Encoding { encoding, mapping }
    }

    pub fn resolve<'a>(encoding: &'a Encoding, mapping: &'a T) -> String {
        format!(
            "{}{}",
            mapping.prefix_to_str(encoding.prefix()),
            encoding.suffix()
        )
    }
}

impl Deref for Encoding {
    type Target = WireEncoding;

    fn deref(&self) -> &Self::Target {
        &self.encoding
    }
}

impl DerefMut for Encoding {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.encoding
    }
}

impl From<WireEncoding> for Encoding {
    fn from(encoding: WireEncoding) -> Self {
        Self {
            encoding,
            mapping: DefaultEncodingMapping,
        }
    }
}

impl From<Encoding> for WireEncoding {
    fn from(encoding: Encoding) -> Self {
        encoding.encoding
    }
}

// Default encoding provided with Zenoh
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefaultEncodingMapping;

use phf::phf_ordered_map;
impl DefaultEncodingMapping {
    /// Prefixes from 0 to 63 are reserved by Zenoh
    /// Users are free to use any prefix from 64 to 255
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
            None => Self::EMPTY,
        }
    }

    // Parse a string into a valid WireEncoding. This functions performs the necessary
    // prefix mapping and suffix substring when parsing the input.
    fn parse(t: &str) -> ZResult<WireEncoding> {
        for (s, p) in Self::KNOWN_STRING.entries() {
            if let Some((_, b)) = t.split_once(s) {
                let a = b.to_string();
                return WireEncoding::new(*p).with_suffix(a);
            }
        }
        // WireEncoding::empty().with_suffix(t)
        Ok(WireEncoding::empty())
    }
}

impl Encoding {
    /// The encoding is empty. It is equivalent to not being defined.
    pub const EMPTY: Self = Self::new(DefaultEncodingMapping::EMPTY);
    pub const APP_OCTET_STREAM: Self = Self::new(DefaultEncodingMapping::APP_OCTET_STREAM);
    pub const APP_CUSTOM: Self = Self::new(DefaultEncodingMapping::APP_CUSTOM);
    pub const TEXT_PLAIN: Self = Self::new(DefaultEncodingMapping::TEXT_PLAIN);
    pub const APP_PROPERTIES: Self = Self::new(DefaultEncodingMapping::APP_PROPERTIES);
    pub const APP_JSON: Self = Self::new(DefaultEncodingMapping::APP_JSON);
    pub const APP_SQL: Self = Self::new(DefaultEncodingMapping::APP_SQL);
    pub const APP_INTEGER: Self = Self::new(DefaultEncodingMapping::APP_INTEGER);
    pub const APP_FLOAT: Self = Self::new(DefaultEncodingMapping::APP_FLOAT);
    pub const APP_XML: Self = Self::new(DefaultEncodingMapping::APP_XML);
    pub const APP_XHTML_XML: Self = Self::new(DefaultEncodingMapping::APP_XHTML_XML);
    pub const APP_XWWW_FORM_URLENCODED: Self =
        Self::new(DefaultEncodingMapping::APP_XWWW_FORM_URLENCODED);
    pub const TEXT_JSON: Self = Self::new(DefaultEncodingMapping::TEXT_JSON);
    pub const TEXT_HTML: Self = Self::new(DefaultEncodingMapping::TEXT_HTML);
    pub const TEXT_XML: Self = Self::new(DefaultEncodingMapping::TEXT_XML);
    pub const TEXT_CSS: Self = Self::new(DefaultEncodingMapping::TEXT_CSS);
    pub const TEXT_CSV: Self = Self::new(DefaultEncodingMapping::TEXT_CSV);
    pub const TEXT_JAVASCRIPT: Self = Self::new(DefaultEncodingMapping::TEXT_JAVASCRIPT);
    pub const IMAGE_JPEG: Self = Self::new(DefaultEncodingMapping::IMAGE_JPEG);
    pub const IMAGE_PNG: Self = Self::new(DefaultEncodingMapping::IMAGE_PNG);
    pub const IMAGE_GIF: Self = Self::new(DefaultEncodingMapping::IMAGE_GIF);

    /// Returns a new [`Encoding`] object.
    pub const fn new(prefix: EncodingPrefix) -> Self {
        Self::exact(prefix, DefaultEncodingMapping)
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl<T> FromStr for Encoding<T>
where
    T: EncodingMapping + Default,
{
    type Err = zenoh_result::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let encoding: WireEncoding = T::parse(s)?;
        Ok(Self {
            encoding,
            mapping: T::default(),
        })
    }
}

impl<T> TryFrom<&str> for Encoding<T>
where
    T: EncodingMapping + Default,
{
    type Error = zenoh_result::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl<T> TryFrom<String> for Encoding<T>
where
    T: EncodingMapping + Default,
{
    type Error = zenoh_result::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}
