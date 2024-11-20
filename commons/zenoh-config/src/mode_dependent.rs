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
use zenoh_protocol::core::{EndPoint, WhatAmI, WhatAmIMatcher, WhatAmIMatcherVisitor};

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
    fn get_mut(&mut self, whatami: WhatAmI) -> Option<&mut T>;
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

    #[inline]
    fn get_mut(&mut self, whatami: WhatAmI) -> Option<&mut T> {
        match whatami {
            WhatAmI::Router => self.router.as_mut(),
            WhatAmI::Peer => self.peer.as_mut(),
            WhatAmI::Client => self.client.as_mut(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ModeDependentValue<T> {
    Unique(T),
    Dependent(ModeValues<T>),
}

impl<T> ModeDependentValue<T> {
    #[inline]
    pub fn set(&mut self, value: T) -> Result<ModeDependentValue<T>, ModeDependentValue<T>> {
        let mut value = ModeDependentValue::Unique(value);
        std::mem::swap(self, &mut value);
        Ok(value)
    }
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

    #[inline]
    fn get_mut(&mut self, whatami: WhatAmI) -> Option<&mut T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.get_mut(whatami),
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

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value as i64))
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

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value as f64))
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

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                WhatAmIMatcherVisitor {}
                    .visit_seq(seq)
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

impl<'a> serde::Deserialize<'a> for ModeDependentValue<Vec<EndPoint>> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<Vec<EndPoint>>> {
            type Value = ModeDependentValue<Vec<EndPoint>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("list of endpoints or mode dependent list of endpoints")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut v = seq.size_hint().map_or_else(Vec::new, Vec::with_capacity);

                while let Some(s) = seq.next_element()? {
                    v.push(s);
                }
                Ok(ModeDependentValue::Unique(v))
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
        self.as_ref().and_then(|m| m.router())
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        self.as_ref().and_then(|m| m.peer())
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        self.as_ref().and_then(|m| m.client())
    }

    #[inline]
    fn get_mut(&mut self, whatami: WhatAmI) -> Option<&mut T> {
        self.as_mut().and_then(|m| m.get_mut(whatami))
    }
}
