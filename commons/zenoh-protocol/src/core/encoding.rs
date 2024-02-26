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
use core::fmt::Debug;
use zenoh_result::{bail, ZResult};

pub type EncodingPrefix = u16;

/// [`Encoding`] is a metadata that indicates how the data payload should be interpreted.
/// For wire-efficiency and extensibility purposes, Zenoh defines an [`Encoding`] as
/// composed of an unsigned integer prefix and a string suffix. The actual meaning of the
/// prefix and suffix are out-of-scope of the protocol definition. Therefore, Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like.
/// Nevertheless, it is worth highlighting that Zenoh still provides a default mapping as part
/// of the API as per user convenience. That mapping has no impact on the Zenoh protocol definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding {
    prefix: EncodingPrefix,
    suffix: CowStr<'static>,
}

/// # Encoding field
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~ prefix: z16 |S~
/// +---------------+
/// ~suffix: <u8;z8>~  -- if S==1
/// +---------------+
/// ```
pub mod flag {
    pub const S: u32 = 1; // 0x01 Suffix    if S==1 then suffix is present
}

impl Encoding {
    pub const DEFAULT: Self = Self::empty();

    /// Returns a new [`Encoding`] object provided the prefix ID.
    pub const fn new(prefix: EncodingPrefix) -> Self {
        Self {
            prefix,
            suffix: CowStr::borrowed(""),
        }
    }

    /// Sets the suffix of the encoding.
    /// It will return an error when the suffix is longer than 255 characters.
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

    /// Returns a new [`Encoding`] object with default empty prefix ID.
    pub const fn empty() -> Self {
        Self::new(0)
    }

    // Returns the numerical prefix
    pub const fn prefix(&self) -> EncodingPrefix {
        self.prefix
    }

    // Returns the suffix string
    pub fn suffix(&self) -> &str {
        self.suffix.as_str()
    }

    /// Returns `true` if the string representation of this encoding starts with
    /// the string representation of the other given encoding.
    pub fn starts_with(&self, with: &Encoding) -> bool {
        self.prefix() == with.prefix() && self.suffix().starts_with(with.suffix())
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
