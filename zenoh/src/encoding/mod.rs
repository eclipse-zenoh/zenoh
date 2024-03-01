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

use crate::value::Value;
pub use default::*;
use std::borrow::Cow;
pub use zenoh_protocol::core::{Encoding, EncodingPrefix};
use zenoh_result::ZResult;

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

pub trait ZEncoding {}

/// Trait to encode a type `T` into a [`Value`].
pub trait Encoder<T> {
    type Output;

    /// The implementer should take care of serializing the type `T` and set the proper [`Encoding`].
    fn encode(self, t: T) -> Self::Output;
}

pub trait Decoder<T> {
    type Error;

    /// The implementer should take care of deserializing the type `T` based on the [`Encoding`] information.
    fn decode(self, t: &Value) -> Result<T, Self::Error>;
}
