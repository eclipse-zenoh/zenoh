use std::{fmt, marker::PhantomData};

use nonempty_collections::NEVec;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Serialize,
};
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher};

use crate::{Interface, ModeDependentValue, ModeValues, ZenohId};

impl<'de> Deserialize<'de> for ModeDependentValue<GatewayConf> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<GatewayConf>> {
            type Value = ModeDependentValue<GatewayConf>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("gateway config or mode dependent gateway config")
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let value =
                    serde_json::Value::deserialize(de::value::MapAccessDeserializer::new(map))?;

                if let Ok(values) = ModeValues::deserialize(&value) {
                    return Ok(ModeDependentValue::Dependent(values));
                }

                Ok(ModeDependentValue::Unique(
                    GatewayConf::deserialize(&value).map_err(de::Error::custom)?,
                ))
            }
        }

        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl Default for ModeDependentValue<GatewayConf> {
    fn default() -> Self {
        ModeDependentValue::Dependent(ModeValues {
            router: Some(GatewayConf {
                north: BoundConf {
                    mode: WhatAmI::Router,
                    filters: Some(vec![BoundFilterConf {
                        modes: Some(WhatAmIMatcher::empty().router()),
                        interfaces: None,
                        zids: None,
                    }]),
                },
                south: vec![BoundConf {
                    mode: WhatAmI::Peer,
                    filters: Some(vec![BoundFilterConf {
                        modes: Some(WhatAmIMatcher::empty().peer().client()),
                        interfaces: None,
                        zids: None,
                    }]),
                }],
            }),
            peer: Some(GatewayConf {
                north: BoundConf {
                    mode: WhatAmI::Peer,
                    filters: Some(vec![BoundFilterConf {
                        modes: Some(WhatAmIMatcher::empty().peer().router()),
                        interfaces: None,
                        zids: None,
                    }]),
                },
                south: vec![BoundConf {
                    mode: WhatAmI::Client,
                    filters: Some(vec![BoundFilterConf {
                        modes: Some(WhatAmIMatcher::empty().client()),
                        interfaces: None,
                        zids: None,
                    }]),
                }],
            }),
            client: Some(GatewayConf {
                north: BoundConf {
                    mode: WhatAmI::Client,
                    filters: None,
                },
                south: vec![],
            }),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConf {
    pub north: BoundConf,
    pub south: Vec<BoundConf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundConf {
    pub mode: WhatAmI,
    pub filters: Option<Vec<BoundFilterConf>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundFilterConf {
    pub modes: Option<WhatAmIMatcher>,
    pub interfaces: Option<NEVec<Interface>>,
    pub zids: Option<NEVec<ZenohId>>,
}
