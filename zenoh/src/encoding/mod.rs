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
pub mod default;
pub mod iana;

pub use default::*;

use crate::value::Value;
use std::borrow::Cow;
use zenoh_protocol::core::{Encoding, EncodingPrefix};
use zenoh_result::ZResult;

pub trait EncodingMapping {
    /// Map a numerical prefix to its string representation
    fn prefix_to_str(&self, e: EncodingPrefix) -> &str;
    /// Map a string to a known numerical prefix ID
    fn str_to_prefix(&self, s: &str) -> EncodingPrefix;

    /// Parse a string into a valid
    fn parse<S>(&self, s: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>;
    fn to_str<'a>(&self, e: &'a Encoding) -> Cow<'a, str>;
}

// Encoder
pub trait Encoder<T> {
    fn encode(t: T) -> Value;
}

pub trait Decoder<T> {
    fn decode(t: &Value) -> ZResult<T>;
}
