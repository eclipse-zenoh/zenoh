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
use crate::core::CowStr;
use alloc::{borrow::Cow, string::String};
use core::fmt::{self, Debug};
use zenoh_result::{bail, ZResult};

pub type EncodingPrefix = u8;

/// The encoding of a zenoh `zenoh::Value`.
///
/// A zenoh encoding is a HTTP Mime type represented, for wire efficiency,
/// as an integer prefix (that maps to a string) and a string suffix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding {
    prefix: EncodingPrefix,
    suffix: CowStr<'static>,
}

pub mod prefix {
    use crate::core::encoding::EncodingPrefix;
    use phf::phf_ordered_map;

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
    pub(super) const KNOWN: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        0u8  =>  "",
        1u8  =>  "application/octet-stream",
        2u8  =>  "application/custom", // non iana standard
        3u8  =>  "text/plain",
        4u8  =>  "application/properties", // non iana standard
        5u8  =>  "application/json", // if not readable from casual users
        6u8  =>  "application/sql",
        7u8  =>  "application/integer", // non iana standard
        8u8  =>  "application/float", // non iana standard
        9u8  =>  "application/xml", // if not readable from casual users (RFC 3023, sec 3)
        10u8  => "application/xhtml+xml",
        11u8  => "application/x-www-form-urlencoded",
        12u8  => "text/json", // non iana standard - if readable from casual users
        13u8  => "text/html",
        14u8  => "text/xml", // if readable from casual users (RFC 3023, section 3)
        15u8  => "text/css",
        16u8  => "text/csv",
        17u8  => "text/javascript",
        18u8  => "image/jpeg",
        19u8  => "image/png",
        20u8  => "image/gif",
    };
}

impl Encoding {
    /// The encoding is empty. It is equivalent to not being defined.
    pub const EMPTY: Encoding = Encoding::exact(prefix::EMPTY);
    pub const APP_OCTET_STREAM: Encoding = Encoding::exact(prefix::APP_OCTET_STREAM);
    pub const APP_CUSTOM: Encoding = Encoding::exact(prefix::APP_CUSTOM);
    pub const TEXT_PLAIN: Encoding = Encoding::exact(prefix::TEXT_PLAIN);
    pub const APP_PROPERTIES: Encoding = Encoding::exact(prefix::APP_PROPERTIES);
    pub const APP_JSON: Encoding = Encoding::exact(prefix::APP_JSON);
    pub const APP_SQL: Encoding = Encoding::exact(prefix::APP_SQL);
    pub const APP_INTEGER: Encoding = Encoding::exact(prefix::APP_INTEGER);
    pub const APP_FLOAT: Encoding = Encoding::exact(prefix::APP_FLOAT);
    pub const APP_XML: Encoding = Encoding::exact(prefix::APP_XML);
    pub const APP_XHTML_XML: Encoding = Encoding::exact(prefix::APP_XHTML_XML);
    pub const APP_XWWW_FORM_URLENCODED: Encoding =
        Encoding::exact(prefix::APP_XWWW_FORM_URLENCODED);
    pub const TEXT_JSON: Encoding = Encoding::exact(prefix::TEXT_JSON);
    pub const TEXT_HTML: Encoding = Encoding::exact(prefix::TEXT_HTML);
    pub const TEXT_XML: Encoding = Encoding::exact(prefix::TEXT_XML);
    pub const TEXT_CSS: Encoding = Encoding::exact(prefix::TEXT_CSS);
    pub const TEXT_CSV: Encoding = Encoding::exact(prefix::TEXT_CSV);
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::exact(prefix::TEXT_JAVASCRIPT);
    pub const IMAGE_JPEG: Encoding = Encoding::exact(prefix::IMAGE_JPEG);
    pub const IMAGE_PNG: Encoding = Encoding::exact(prefix::IMAGE_PNG);
    pub const IMAGE_GIF: Encoding = Encoding::exact(prefix::IMAGE_GIF);

    pub const fn exact(prefix: EncodingPrefix) -> Self {
        Self {
            prefix,
            suffix: CowStr::borrowed(""),
        }
    }

    /// Returns a new [`Encoding`] object.
    /// This method will return an error when the suffix is longer than 255 characters.
    pub fn new<IntoCowStr>(prefix: EncodingPrefix, suffix: IntoCowStr) -> ZResult<Self>
    where
        IntoCowStr: Into<Cow<'static, str>> + AsRef<str>,
    {
        let suffix: Cow<'static, str> = suffix.into();
        if suffix.as_bytes().len() > EncodingPrefix::MAX as usize {
            bail!("Suffix length is limited to 255 characters")
        }
        Ok(Self {
            prefix,
            suffix: suffix.into(),
        })
    }

    /// Sets the suffix of this encoding.
    /// This method will return an error when the suffix is longer than 255 characters.
    pub fn with_suffix<IntoCowStr>(mut self, suffix: IntoCowStr) -> ZResult<Self>
    where
        IntoCowStr: Into<Cow<'static, str>> + AsRef<str>,
    {
        let s: Cow<'static, str> = suffix.into();
        if s.as_bytes().len() > u8::MAX as usize {
            bail!("Suffix length is limited to 255 characters")
        }
        self.suffix = (self.suffix + s.as_ref()).into();
        Ok(self)
    }

    pub fn as_ref<'a, T>(&'a self) -> T
    where
        &'a Self: Into<T>,
    {
        self.into()
    }

    /// Returns `true` if the string representation of this encoding starts with
    /// the string representation of ther given encoding.
    pub fn starts_with<T>(&self, with: T) -> bool
    where
        T: Into<Encoding>,
    {
        let with: Encoding = with.into();
        self.prefix() == with.prefix() && self.suffix().starts_with(with.suffix())
    }

    pub const fn prefix(&self) -> EncodingPrefix {
        self.prefix
    }

    pub fn suffix(&self) -> &str {
        self.suffix.as_str()
    }
}

impl From<&EncodingPrefix> for Encoding {
    fn from(e: &EncodingPrefix) -> Encoding {
        Encoding::exact(*e)
    }
}

impl From<EncodingPrefix> for Encoding {
    fn from(e: EncodingPrefix) -> Encoding {
        Encoding::exact(e)
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match prefix::KNOWN.get(&self.prefix) {
            Some(s) => write!(f, "{}", s)?,
            None => write!(f, "Unknown({})", self.prefix)?,
        }
        write!(f, "{}", self.suffix.as_str())
    }
}

impl TryFrom<&'static str> for Encoding {
    type Error = zenoh_result::Error;

    fn try_from(s: &'static str) -> Result<Self, Self::Error> {
        for (k, v) in prefix::KNOWN.entries().skip(1) {
            if let Some(suffix) = s.strip_prefix(v) {
                let e = Encoding::exact(*k);
                if suffix.is_empty() {
                    return Ok(e);
                } else {
                    return e.with_suffix(suffix);
                }
            }
        }

        let e = Encoding::EMPTY;
        if s.is_empty() {
            Ok(e)
        } else {
            e.with_suffix(s)
        }
    }
}

impl TryFrom<String> for Encoding {
    type Error = zenoh_result::Error;

    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        for (k, v) in prefix::KNOWN.entries().skip(1) {
            if s.starts_with(v) {
                s.replace_range(..v.len(), "");
                let e = Encoding::exact(*k);
                if s.is_empty() {
                    return Ok(e);
                } else {
                    return e.with_suffix(s);
                }
            }
        }

        let e = Encoding::EMPTY;
        if s.is_empty() {
            Ok(e)
        } else {
            e.with_suffix(s)
        }
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::EMPTY
    }
}

impl Encoding {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::{
            distributions::{Alphanumeric, DistString},
            Rng,
        };

        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let prefix: EncodingPrefix = rng.gen();
        let suffix: String = if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            Alphanumeric.sample_string(&mut rng, len)
        } else {
            String::new()
        };
        Encoding::new(prefix, suffix).unwrap()
    }
}
