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
use alloc::borrow::Cow;
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

impl Encoding {
    /// Returns a new [`WireEncoding`] object provided the prefix ID.
    pub const fn new(prefix: EncodingPrefix) -> Self {
        Self {
            prefix,
            suffix: CowStr::borrowed(""),
        }
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

    /// Returns a new [`WireEncoding`] object with default empty prefix ID.
    pub const fn empty() -> Self {
        Self::new(0)
    }

    // Returns the numerical prefix ID
    pub const fn prefix(&self) -> EncodingPrefix {
        self.prefix
    }

    // Returns the suffix string
    pub fn suffix(&self) -> &str {
        self.suffix.as_str()
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
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{}:{}", self.prefix, self.suffix.as_str()))
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::empty()
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
        Encoding::new(prefix).with_suffix(suffix).unwrap()
    }
}
