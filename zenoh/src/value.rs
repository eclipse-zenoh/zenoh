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
use std::borrow::Cow;
use zenoh_protocol::core::EncodingPrefix;
use zenoh_result::ZResult;

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

pub mod prefix {
    use phf::phf_ordered_map;
    use zenoh_protocol::core::EncodingPrefix;

    // - Primitives types supported in all Zenoh bindings.

    /// Unspecified [`EncodingPrefix`].
    /// Note that an [`Encoding`] could have an empty [prefix](`Encoding::prefix`) and a non-empty [suffix](`Encoding::suffix`).
    pub const EMPTY: EncodingPrefix = 0;
    /// A stream of bytes.
    pub const APPLICATION_OCTET_STREAM: EncodingPrefix = 1;
    /// A signed integer.
    pub const ZENOH_INT: EncodingPrefix = 2;
    /// An unsigned integer.
    pub const ZENOH_UINT: EncodingPrefix = 3;
    /// A float.
    pub const ZENOH_FLOAT: EncodingPrefix = 4;
    /// A boolean.
    pub const ZENOH_BOOL: EncodingPrefix = 5;
    /// A string.
    pub const TEXT_PLAIN: EncodingPrefix = 6;

    // - Advanced types supported in some Zenoh bindings.

    /// A JSON intended to be consumed by an application.
    pub const APPLICATION_JSON: EncodingPrefix = 7;
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: EncodingPrefix = 8;

    // - 9-15 are reserved

    // - List of known mapping. Encoding capabilities may not be provided at all.

    /// A Common Data Representation (CDR)-encoded data. A [suffix](`Encoding::suffix`) may be provided in the [`Encoding`] to specify the concrete type.
    pub const APPLICATION_CDR: EncodingPrefix = 16;

    // - 17-63 are reserved
    // - The highest prefix value to fit in 1 byte on the wire is 63.
    // - 64-1014 are reserved.

    // - A list of known prefixes. Encoding capabilities may not be provided at all.

    /// Common prefix for Zenoh-defined types.
    pub const ZENOH: EncodingPrefix = 1_014;

    // - A list of IANA registries.

    /// Common prefix for *application* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#application).
    pub const APPLICATION: EncodingPrefix = 1_015;
    /// Common prefix for *audio* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#audio).
    pub const AUDIO: EncodingPrefix = 1_016;
    /// Common prefix for *font* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#font).
    pub const FONT: EncodingPrefix = 1_017;
    /// Common prefix for *image* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#image).
    pub const IMAGE: EncodingPrefix = 1_018;
    /// Common prefix for *message* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#message).
    pub const MESSAGE: EncodingPrefix = 1_019;
    /// Common prefix for *model* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#model).
    pub const MODEL: EncodingPrefix = 1_020;
    /// Common prefix for *multipart* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#multipart).
    pub const MULTIPART: EncodingPrefix = 1_021;
    /// Common prefix for *text* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#text).
    pub const TEXT: EncodingPrefix = 1_022;
    /// Common prefix for *video* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#video).
    pub const VIDEO: EncodingPrefix = 1_023;

    // - 1024-65535 are free to use.

    // - End encoding prefix definition

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
        16u16 =>  "application/cdr",
        // - 17-1019 are reserved.
        1_014u16 => "zenoh/",
        1_015u16 => "application/",
        1_016u16 => "audio/",
        1_017u16 => "font/",
        1_018u16 => "image/",
        1_019u16 => "message/",
        1_020u16 => "model/",
        1_021u16 => "multipart/",
        1_022u16 => "text/",
        1_023u16 => "video/",
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
        "application/cdr" =>  16u16,
        // - 17-1019 are reserved.
        "zenoh/" => 1_014u16,
        "application/" => 1_015u16,
        "audio/" => 1_016u16,
        "font/" => 1_017u16,
        "image/" => 1_018u16,
        "message/" => 1_019u16,
        "model/" => 1_020u16,
        "multipart/" => 1_021u16,
        "text/" => 1_022u16,
        "video/" => 1_023u16,
        // - 1024-65535 are free to use.
    };
}

impl DefaultEncoding {
    // - Primitives types supported in all Zenoh bindings

    /// See [`DefaultEncodingMapping::EMPTY`].
    pub const EMPTY: Encoding = Encoding::new(prefix::EMPTY);
    /// An application-specific stream of bytes.
    pub const APPLICATION_OCTET_STREAM: Encoding = Encoding::new(prefix::APPLICATION_OCTET_STREAM);
    /// A VLE-encoded signed little-endian integer. Either 8bit, 16bit, 32bit, or 64bit.
    /// Binary reprensentation uses two's complement.
    pub const ZENOH_INT: Encoding = Encoding::new(prefix::ZENOH_INT);
    /// A VLE-encoded little-endian unsigned integer. Either 8bit, 16bit, 32bit, or 64bit.
    pub const ZENOH_UINT: Encoding = Encoding::new(prefix::ZENOH_UINT);
    /// A VLE-encoded float. Either little-endian 32bit or 64bit.
    /// Binary representation uses *IEEE 754-2008* *binary32* or *binary64*, respectively.
    pub const ZENOH_FLOAT: Encoding = Encoding::new(prefix::ZENOH_FLOAT);
    /// A boolean. `0` is `false`, `1` is `true`. Other values are invalid.
    pub const ZENOH_BOOL: Encoding = Encoding::new(prefix::ZENOH_BOOL);
    /// A UTF-8 encoded string.
    pub const TEXT_PLAIN: Encoding = Encoding::new(prefix::TEXT_PLAIN);

    // - Advanced types supported in some Zenoh bindings.

    /// A JSON intended to be consumed by an application.
    pub const APPLICATION_JSON: Encoding = Encoding::new(prefix::APPLICATION_JSON);
    /// A JSON intended to be human readable.
    pub const TEXT_JSON: Encoding = Encoding::new(prefix::TEXT_JSON);

    // - List of known mapping. Encoding capabilities may not be provided at all.

    /// A Common Data Representation (CDR)-encoded data. A [suffix](`Encoding::suffix`) may be provided in the [`Encoding`] to specify the concrete type.
    pub const APPLICATION_CDR: Encoding = Encoding::new(prefix::APPLICATION_CDR);

    // - A list of known prefixes.

    /// Common prefix for Zenoh-defined types.
    pub const ZENOH: Encoding = Encoding::new(prefix::ZENOH);

    // - A list of IANA registries.

    /// Common prefix for *application* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#application).
    pub const APPLICATION: Encoding = Encoding::new(prefix::APPLICATION);
    /// Common prefix for *audio* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#audio).
    pub const AUDIO: Encoding = Encoding::new(prefix::AUDIO);
    /// Common prefix for *font* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#font).
    pub const FONT: Encoding = Encoding::new(prefix::FONT);
    /// Common prefix for *image* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#image).
    pub const IMAGE: Encoding = Encoding::new(prefix::IMAGE);
    /// Common prefix for *message* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#message).
    pub const MESSAGE: Encoding = Encoding::new(prefix::MESSAGE);
    /// Common prefix for *model* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#model).
    pub const MODEL: Encoding = Encoding::new(prefix::MODEL);
    /// Common prefix for *multipart* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#multipart).
    pub const MULTIPART: Encoding = Encoding::new(prefix::MULTIPART);
    /// Common prefix for *text* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#text).
    pub const TEXT: Encoding = Encoding::new(prefix::TEXT);
    /// Common prefix for *video* MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml#video).
    pub const VIDEO: Encoding = Encoding::new(prefix::VIDEO);
}

/// Trait to create, resolve, parse an [`Encoding`] mapping.
pub trait EncodingMapping {
    // The minimum prefix used by the EncodingMapping implementer
    const MIN: EncodingPrefix;
    // The maximum prefix used by the EncodingMapping implementer
    const MAX: EncodingPrefix;

    /// Map a numerical prefix to its string representation.
    fn prefix_to_str(&self, e: EncodingPrefix) -> Option<Cow<'_, str>>;
    /// Map a string to a known numerical prefix ID.
    fn str_to_prefix(&self, s: &str) -> Option<EncodingPrefix>;
    /// Parse a string into a valid [`Encoding`].
    fn parse<S>(&self, s: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>;
    fn to_str(&self, e: &Encoding) -> Cow<'_, str>;
}

impl EncodingMapping for DefaultEncoding {
    const MIN: EncodingPrefix = 0;
    const MAX: EncodingPrefix = 1023;

    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> Option<Cow<'static, str>> {
        prefix::KNOWN_PREFIX.get(&p).map(|s| Cow::Borrowed(*s))
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`DefaultEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> Option<EncodingPrefix> {
        prefix::KNOWN_STRING.get(s).copied()
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`DefaultEncodingMapping::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse(_self: &DefaultEncoding, t: Cow<'static, str>) -> ZResult<Encoding> {
            // Check if empty
            if t.is_empty() {
                return Ok(DefaultEncoding::EMPTY);
            }
            // Try first an exact lookup of the string to prefix
            if let Some(p) = _self.str_to_prefix(t.as_ref()) {
                return Ok(Encoding::new(p));
            }
            // Check if the passed string matches one of the known prefixes. It will map the known string
            // prefix to the numerical prefix and carry the remaining part of the string in the suffix.
            // Skip empty string mapping. The order is guaranteed by the phf::OrderedMap.
            for (s, p) in prefix::KNOWN_STRING.entries().skip(1) {
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
    fn to_str(&self, e: &Encoding) -> Cow<'_, str> {
        let (p, s) = (e.prefix(), e.suffix());
        match self.prefix_to_str(p) {
            Some(p) if s.is_empty() => p,
            Some(p) => Cow::Owned(format!("{}{}", p, s)),
            None => Cow::Owned(format!("unknown({}){}", p, s)),
        }
    }
}
