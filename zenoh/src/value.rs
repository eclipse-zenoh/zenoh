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
use crate::{encoding::Encoding, payload::Payload, sample::builder::ValueBuilderTrait};

/// A zenoh [`Value`] contains a `payload` and an [`Encoding`] that indicates how the [`Payload`] should be interpreted.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    /// The binary [`Payload`] of this [`Value`].
    pub payload: Payload,
    /// The [`Encoding`] of this [`Value`].
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new [`Value`] with default [`Encoding`].
    pub fn new<T>(payload: T) -> Self
    where
        T: Into<Payload>,
    {
        Value {
            payload: payload.into(),
            encoding: Encoding::default(),
        }
    }
    /// Creates an empty [`Value`].
    pub const fn empty() -> Self {
        Value {
            payload: Payload::empty(),
            encoding: Encoding::default(),
        }
    }
}

impl ValueBuilderTrait for Value {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            ..self
        }
    }
    fn payload<T: Into<Payload>>(self, payload: T) -> Self {
        Self {
            payload: payload.into(),
            ..self
        }
    }
}

impl<T> From<T> for Value
where
    T: Into<Payload>,
{
    fn from(t: T) -> Self {
        Value {
            payload: t.into(),
            encoding: Encoding::default(),
        }
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::empty()
    }
}
