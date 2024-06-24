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

use std::{fmt, marker::PhantomData};

use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Serialize,
};
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, WhatAmIMatcherVisitor};

pub trait ModeDependent<T> {
    fn router(&self) -> Option<&T>;
    fn peer(&self) -> Option<&T>;
    fn client(&self) -> Option<&T>;
    #[inline]
    fn get(&self, whatami: WhatAmI) -> Option<&T> {
        match whatami {
            WhatAmI::Router => self.router(),
            WhatAmI::Peer => self.peer(),
            WhatAmI::Client => self.client(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModeValues<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<T>,
}

impl<T> ModeDependent<T> for ModeValues<T> {
    #[inline]
    fn router(&self) -> Option<&T> {
        self.router.as_ref()
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        self.peer.as_ref()
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        self.client.as_ref()
    }
}

#[derive(Clone, Debug)]
pub enum ModeDependentValue<T> {
    Unique(T),
    Dependent(ModeValues<T>),
}

impl<T> ModeDependent<T> for ModeDependentValue<T> {
    #[inline]
    fn router(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.router(),
        }
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.peer(),
        }
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.client(),
        }
    }
}

impl<T> serde::Serialize for ModeDependentValue<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ModeDependentValue::Unique(value) => value.serialize(serializer),
            ModeDependentValue::Dependent(options) => options.serialize(serializer),
        }
    }
}

impl<'a> serde::Deserialize<'a> for ModeDependentValue<bool> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<bool>> {
            type Value = ModeDependentValue<bool>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("bool or mode dependent bool")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<'a> serde::Deserialize<'a> for ModeDependentValue<i64> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<i64>> {
            type Value = ModeDependentValue<i64>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("i64 or mode dependent i64")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<'a> serde::Deserialize<'a> for ModeDependentValue<f64> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<f64>> {
            type Value = ModeDependentValue<f64>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("f64 or mode dependent f64")
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value as f64))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<'a> serde::Deserialize<'a> for ModeDependentValue<WhatAmIMatcher> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<WhatAmIMatcher>> {
            type Value = ModeDependentValue<WhatAmIMatcher>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("WhatAmIMatcher or mode dependent WhatAmIMatcher")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                WhatAmIMatcherVisitor {}
                    .visit_str(value)
                    .map(ModeDependentValue::Unique)
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<T> ModeDependent<T> for Option<ModeDependentValue<T>> {
    #[inline]
    fn router(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.router(),
            None => None,
        }
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.peer(),
            None => None,
        }
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.client(),
            None => None,
        }
    }
}
