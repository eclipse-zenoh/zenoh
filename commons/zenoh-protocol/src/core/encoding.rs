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
use core::{
    convert::TryFrom,
    fmt::{self, Debug},
    mem,
};
use zenoh_result::{bail, zerror, ZError, ZResult};

mod consts {
    pub(super) const MIMES: [&str; 21] = [
        /*  0 */ "",
        /*  1 */ "application/octet-stream",
        /*  2 */ "application/custom", // non iana standard
        /*  3 */ "text/plain",
        /*  4 */ "application/properties", // non iana standard
        /*  5 */ "application/json", // if not readable from casual users
        /*  6 */ "application/sql",
        /*  7 */ "application/integer", // non iana standard
        /*  8 */ "application/float", // non iana standard
        /*  9 */
        "application/xml", // if not readable from casual users (RFC 3023, sec 3)
        /* 10 */ "application/xhtml+xml",
        /* 11 */ "application/x-www-form-urlencoded",
        /* 12 */ "text/json", // non iana standard - if readable from casual users
        /* 13 */ "text/html",
        /* 14 */ "text/xml", // if readable from casual users (RFC 3023, section 3)
        /* 15 */ "text/css",
        /* 16 */ "text/csv",
        /* 17 */ "text/javascript",
        /* 18 */ "image/jpeg",
        /* 19 */ "image/png",
        /* 20 */ "image/gif",
    ];
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// tags{rust.encoding, api.options.encoding}
pub enum KnownEncoding {
    Empty = 0,
    AppOctetStream = 1,
    AppCustom = 2,
    TextPlain = 3,
    AppProperties = 4,
    AppJson = 5,
    AppSql = 6,
    AppInteger = 7,
    AppFloat = 8,
    AppXml = 9,
    AppXhtmlXml = 10,
    AppXWwwFormUrlencoded = 11,
    TextJson = 12,
    TextHtml = 13,
    TextXml = 14,
    TextCss = 15,
    TextCsv = 16,
    TextJavascript = 17,
    ImageJpeg = 18,
    ImagePng = 19,
    ImageGif = 20,
}

impl From<KnownEncoding> for u8 {
    fn from(val: KnownEncoding) -> Self {
        val as u8
    }
}

impl From<KnownEncoding> for &str {
    fn from(val: KnownEncoding) -> Self {
        consts::MIMES[u8::from(val) as usize]
    }
}

impl TryFrom<u8> for KnownEncoding {
    type Error = ZError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < consts::MIMES.len() as u8 + 1 {
            Ok(unsafe { mem::transmute(value) })
        } else {
            Err(zerror!("Unknown encoding"))
        }
    }
}

impl AsRef<str> for KnownEncoding {
    fn as_ref(&self) -> &str {
        consts::MIMES[u8::from(*self) as usize]
    }
}

/// The encoding of a zenoh `zenoh::Value`.
///
/// A zenoh encoding is a HTTP Mime type represented, for wire efficiency,
/// as an integer prefix (that maps to a string) and a string suffix.
// tags{rust.encoding, api.encoding}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Encoding {
    // tags{}
    Exact(KnownEncoding),
    // tags{}
    WithSuffix(KnownEncoding, CowStr<'static>),
}

impl Encoding {
    // tags{rust.encoding.new}
    pub fn new<IntoCowStr>(prefix: u8, suffix: IntoCowStr) -> ZResult<Self>
    where
        IntoCowStr: Into<Cow<'static, str>> + AsRef<str>,
    {
        let prefix = KnownEncoding::try_from(prefix)?;
        let suffix = suffix.into();
        if suffix.as_bytes().len() > u8::MAX as usize {
            bail!("Suffix length is limited to 255 characters")
        }
        if suffix.as_ref().is_empty() {
            Ok(Encoding::Exact(prefix))
        } else {
            Ok(Encoding::WithSuffix(prefix, suffix.into()))
        }
    }

    /// Sets the suffix of this encoding.
    // tags{rust.encoding.with_suffix}
    pub fn with_suffix<IntoCowStr>(self, suffix: IntoCowStr) -> ZResult<Self>
    where
        IntoCowStr: Into<Cow<'static, str>> + AsRef<str>,
    {
        match self {
            Encoding::Exact(e) => Encoding::new(e as u8, suffix),
            Encoding::WithSuffix(e, s) => Encoding::new(e as u8, s + suffix.as_ref()),
        }
    }

    // tags{}
    pub fn as_ref<'a, T>(&'a self) -> T
    where
        &'a Self: Into<T>,
    {
        self.into()
    }

    /// Returns `true`if the string representation of this encoding starts with
    /// the string representation of ther given encoding.
    // tags{rust.encoding.starts_with}
    pub fn starts_with<T>(&self, with: T) -> bool
    where
        T: Into<Encoding>,
    {
        let with: Encoding = with.into();
        self.prefix() == with.prefix() && self.suffix().starts_with(with.suffix())
    }

    // tags{rust.encoding.prefix}
    pub const fn prefix(&self) -> &KnownEncoding {
        match self {
            Encoding::Exact(e) | Encoding::WithSuffix(e, _) => e,
        }
    }

    // tags{rust.encoding.suffix}
    pub fn suffix(&self) -> &str {
        match self {
            Encoding::Exact(_) => "",
            Encoding::WithSuffix(_, s) => s.as_ref(),
        }
    }
}

impl Encoding {
    // tags{}
    pub const EMPTY: Encoding = Encoding::Exact(KnownEncoding::Empty);
    // tags{}
    pub const APP_OCTET_STREAM: Encoding = Encoding::Exact(KnownEncoding::AppOctetStream);
    // tags{}
    pub const APP_CUSTOM: Encoding = Encoding::Exact(KnownEncoding::AppCustom);
    // tags{}
    pub const TEXT_PLAIN: Encoding = Encoding::Exact(KnownEncoding::TextPlain);
    // tags{}
    pub const APP_PROPERTIES: Encoding = Encoding::Exact(KnownEncoding::AppProperties);
    // tags{}
    pub const APP_JSON: Encoding = Encoding::Exact(KnownEncoding::AppJson);
    // tags{}
    pub const APP_SQL: Encoding = Encoding::Exact(KnownEncoding::AppSql);
    // tags{}
    pub const APP_INTEGER: Encoding = Encoding::Exact(KnownEncoding::AppInteger);
    // tags{}
    pub const APP_FLOAT: Encoding = Encoding::Exact(KnownEncoding::AppFloat);
    // tags{}
    pub const APP_XML: Encoding = Encoding::Exact(KnownEncoding::AppXml);
    // tags{}
    pub const APP_XHTML_XML: Encoding = Encoding::Exact(KnownEncoding::AppXhtmlXml);
    // tags{}
    pub const APP_XWWW_FORM_URLENCODED: Encoding =
        // tags{}
        Encoding::Exact(KnownEncoding::AppXWwwFormUrlencoded);
    // tags{}
    pub const TEXT_JSON: Encoding = Encoding::Exact(KnownEncoding::TextJson);
    // tags{}
    pub const TEXT_HTML: Encoding = Encoding::Exact(KnownEncoding::TextHtml);
    // tags{}
    pub const TEXT_XML: Encoding = Encoding::Exact(KnownEncoding::TextXml);
    // tags{}
    pub const TEXT_CSS: Encoding = Encoding::Exact(KnownEncoding::TextCss);
    // tags{}
    pub const TEXT_CSV: Encoding = Encoding::Exact(KnownEncoding::TextCsv);
    // tags{}
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::Exact(KnownEncoding::TextJavascript);
    // tags{}
    pub const IMAGE_JPEG: Encoding = Encoding::Exact(KnownEncoding::ImageJpeg);
    // tags{}
    pub const IMAGE_PNG: Encoding = Encoding::Exact(KnownEncoding::ImagePng);
    // tags{}
    pub const IMAGE_GIF: Encoding = Encoding::Exact(KnownEncoding::ImageGif);
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Encoding::Exact(e) => f.write_str(e.as_ref()),
            Encoding::WithSuffix(e, s) => {
                f.write_str(e.as_ref())?;
                f.write_str(s)
            }
        }
    }
}

impl From<&'static str> for Encoding {
    fn from(s: &'static str) -> Self {
        for (i, v) in consts::MIMES.iter().enumerate().skip(1) {
            if let Some(suffix) = s.strip_prefix(v) {
                if suffix.is_empty() {
                    return Encoding::Exact(unsafe { mem::transmute(i as u8) });
                } else {
                    return Encoding::WithSuffix(unsafe { mem::transmute(i as u8) }, suffix.into());
                }
            }
        }
        if s.is_empty() {
            Encoding::Exact(KnownEncoding::Empty)
        } else {
            Encoding::WithSuffix(KnownEncoding::Empty, s.into())
        }
    }
}

impl From<String> for Encoding {
    fn from(mut s: String) -> Self {
        for (i, v) in consts::MIMES.iter().enumerate().skip(1) {
            if s.starts_with(v) {
                s.replace_range(..v.len(), "");
                if s.is_empty() {
                    return Encoding::Exact(unsafe { mem::transmute(i as u8) });
                } else {
                    return Encoding::WithSuffix(unsafe { mem::transmute(i as u8) }, s.into());
                }
            }
        }
        if s.is_empty() {
            Encoding::Exact(KnownEncoding::Empty)
        } else {
            Encoding::WithSuffix(KnownEncoding::Empty, s.into())
        }
    }
}

impl From<&KnownEncoding> for Encoding {
    fn from(e: &KnownEncoding) -> Encoding {
        Encoding::Exact(*e)
    }
}

impl From<KnownEncoding> for Encoding {
    fn from(e: KnownEncoding) -> Encoding {
        Encoding::Exact(e)
    }
}

impl Default for Encoding {
    fn default() -> Self {
        KnownEncoding::Empty.into()
    }
}

impl Encoding {
    #[cfg(feature = "test")]
    // tags{}
    pub fn rand() -> Self {
        use rand::{
            distributions::{Alphanumeric, DistString},
            Rng,
        };

        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let prefix: u8 = rng.gen_range(0..20);
        let suffix: String = if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            Alphanumeric.sample_string(&mut rng, len)
        } else {
            String::new()
        };
        Encoding::new(prefix, suffix).unwrap()
    }
}
