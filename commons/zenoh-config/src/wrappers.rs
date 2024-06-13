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

//! Wrappers around types reexported by `zenoh` from subcrates.
//! These wrappers are used to avoid exposing the the API necessary only for zenoh internals into the public API.

use core::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use zenoh_protocol::{
    core::{EntityGlobalIdProto, EntityId, Locator, WhatAmI, ZenohIdProto},
    scouting::HelloProto,
};

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[repr(transparent)]
pub struct ZenohId(ZenohIdProto);

impl fmt::Debug for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::Display for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<ZenohIdProto> for ZenohId {
    fn from(id: ZenohIdProto) -> Self {
        Self(id)
    }
}

impl From<ZenohId> for ZenohIdProto {
    fn from(id: ZenohId) -> Self {
        id.0
    }
}

impl From<ZenohId> for uhlc::ID {
    fn from(zid: ZenohId) -> Self {
        zid.0.into()
    }
}

impl FromStr for ZenohId {
    type Err = zenoh_result::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ZenohIdProto::from_str(s).map(|zid| zid.into())
    }
}

/// A zenoh Hello message.
#[repr(transparent)]
pub struct Hello(HelloProto);

impl Hello {
    /// Get the locators of this Hello message.
    pub fn locators(&self) -> &[Locator] {
        &self.0.locators
    }

    /// Get the zenoh id of this Hello message.
    pub fn zid(&self) -> ZenohIdProto {
        self.0.zid
    }

    /// Get the whatami of this Hello message.
    pub fn whatami(&self) -> WhatAmI {
        self.0.whatami
    }
}

impl From<HelloProto> for Hello {
    fn from(inner: HelloProto) -> Self {
        Hello(inner)
    }
}

impl fmt::Debug for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Hello")
            .field("zid", &self.zid())
            .field("whatami", &self.whatami())
            .field("locators", &self.locators())
            .finish()
    }
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Default, Copy, Clone, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct EntityGlobalId(EntityGlobalIdProto);

impl EntityGlobalId {
    pub fn zid(&self) -> ZenohId {
        self.0.zid.into()
    }

    pub fn eid(&self) -> EntityId {
        self.0.eid
    }
}

impl fmt::Debug for EntityGlobalId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("EntityGlobalId")
            .field("zid", &self.zid())
            .field("eid", &self.eid())
            .finish()
    }
}

impl From<EntityGlobalIdProto> for EntityGlobalId {
    fn from(id: EntityGlobalIdProto) -> Self {
        Self(id)
    }
}
