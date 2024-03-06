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
use crate::{encoding::Encoding, payload::Payload};

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

    /// Sets the encoding of this [`Value`]`.
    #[inline(always)]
    pub fn with_encoding<IntoEncoding>(mut self, encoding: IntoEncoding) -> Self
    where
        IntoEncoding: Into<Encoding>,
    {
        self.encoding = encoding.into();
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
            encoding: Encoding::default(),
        }
    }
}
