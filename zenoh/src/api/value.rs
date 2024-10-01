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
use super::{bytes::ZBytes, encoding::Encoding};

/// A zenoh [`Value`] contains a `payload` and an [`Encoding`] that indicates how the payload's [`ZBytes`] should be interpreted.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    pub payload: ZBytes,
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new [`Value`] with specified [`ZBytes`] and  [`Encoding`].
    pub fn new<T, E>(payload: T, encoding: E) -> Self
    where
        T: Into<ZBytes>,
        E: Into<Encoding>,
    {
        Value {
            payload: payload.into(),
            encoding: encoding.into(),
        }
    }
    /// Creates an empty [`Value`].
    pub const fn empty() -> Self {
        Value {
            payload: ZBytes::new(),
            encoding: Encoding::default(),
        }
    }
    /// Checks if the [`Value`] is empty.
    /// Value is considered empty if its payload is empty and encoding is default.
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty() && self.encoding == Encoding::default()
    }

    /// Gets binary [`ZBytes`] of this [`Value`].
    pub fn payload(&self) -> &ZBytes {
        &self.payload
    }

    /// Gets [`Encoding`] of this [`Value`].
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }
}

impl<T> From<Option<T>> for Value
where
    T: Into<Value>,
{
    fn from(t: Option<T>) -> Self {
        t.map_or_else(Value::empty, Into::into)
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::empty()
    }
}
