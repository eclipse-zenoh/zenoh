//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
pub mod rname;

use std::convert::From;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
pub use uhlc::Timestamp;
use zenoh_util::core::{ZError, ZErrorKind};
use zenoh_util::zerror;

pub type TimestampId = uhlc::ID;

pub type ZInt = u64;
pub type ZiInt = i64;
pub type AtomicZInt = AtomicU64;
pub const ZINT_MAX_BYTES: usize = 10;

zconfigurable! {
    static ref CONGESTION_CONTROL_DEFAULT: CongestionControl = CongestionControl::Drop;
}

// WhatAmI values
pub type WhatAmI = whatami::Type;
pub mod whatami {
    use super::ZInt;

    pub type Type = ZInt;

    pub const ROUTER: Type = 1; // 0x01
    pub const PEER: Type = 1 << 1; // 0x02
    pub const CLIENT: Type = 1 << 2; // 0x04
                                     // b4-b13: Reserved

    pub fn to_string(w: Type) -> String {
        match w {
            ROUTER => "Router".to_string(),
            PEER => "Peer".to_string(),
            CLIENT => "Client".to_string(),
            i => i.to_string(),
        }
    }
}

pub type ResourceId = ZInt;

pub const NO_RESOURCE_ID: ResourceId = 0;

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~      id       — if ResName{name} : id=0
// +-+-+-+-+-+-+-+-+
// ~  name/suffix  ~ if flag C!=1 in Message's header
// +---------------+
//
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum ResKey {
    RName(String),
    RId(ResourceId),
    RIdWithSuffix(ResourceId, String),
}
use ResKey::*;

impl ResKey {
    #[inline(always)]
    pub fn rid(&self) -> ResourceId {
        match self {
            RName(_) => NO_RESOURCE_ID,
            RId(rid) | RIdWithSuffix(rid, _) => *rid,
        }
    }

    #[inline(always)]
    pub fn is_numerical(&self) -> bool {
        matches!(self, RId(_))
    }
}

impl fmt::Debug for ResKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RName(name) => write!(f, "{}", name),
            RId(rid) => write!(f, "{}", rid),
            RIdWithSuffix(rid, suffix) => write!(f, "{}, {}", rid, suffix),
        }
    }
}

impl fmt::Display for ResKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl From<ResourceId> for ResKey {
    #[inline]
    fn from(rid: ResourceId) -> ResKey {
        RId(rid)
    }
}

impl From<&str> for ResKey {
    #[inline]
    fn from(name: &str) -> ResKey {
        RName(name.to_string())
    }
}

impl From<String> for ResKey {
    #[inline]
    fn from(name: String) -> ResKey {
        RName(name)
    }
}

impl From<(ResourceId, &str)> for ResKey {
    #[inline]
    fn from(tuple: (ResourceId, &str)) -> ResKey {
        if tuple.1.is_empty() {
            RId(tuple.0)
        } else if tuple.0 == NO_RESOURCE_ID {
            RName(tuple.1.to_string())
        } else {
            RIdWithSuffix(tuple.0, tuple.1.to_string())
        }
    }
}

impl From<(ResourceId, String)> for ResKey {
    #[inline]
    fn from(tuple: (ResourceId, String)) -> ResKey {
        if tuple.1.is_empty() {
            RId(tuple.0)
        } else if tuple.0 == NO_RESOURCE_ID {
            RName(tuple.1)
        } else {
            RIdWithSuffix(tuple.0, tuple.1)
        }
    }
}

impl<'a> From<&'a ResKey> for (ResourceId, &'a str) {
    #[inline]
    fn from(key: &'a ResKey) -> (ResourceId, &'a str) {
        match key {
            RId(rid) => (*rid, ""),
            RName(name) => (NO_RESOURCE_ID, &name[..]), //(&(0 as u64)
            RIdWithSuffix(rid, suffix) => (*rid, &suffix[..]),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: ZInt,
    pub value: Vec<u8>,
}

#[derive(Clone, Eq)]
pub struct PeerId {
    size: usize,
    id: [u8; PeerId::MAX_SIZE],
}

impl PeerId {
    pub const MAX_SIZE: usize = 16;

    pub fn new(size: usize, id: [u8; PeerId::MAX_SIZE]) -> PeerId {
        PeerId { size, id }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.id[..self.size]
    }
}

impl From<uuid::Uuid> for PeerId {
    #[inline]
    fn from(uuid: uuid::Uuid) -> Self {
        PeerId {
            size: 16,
            id: *uuid.as_bytes(),
        }
    }
}

impl PartialEq for PeerId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && self.as_slice() == other.as_slice()
    }
}

impl Hash for PeerId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(self.as_slice()))
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// A PeerID can be converted into a Timestamp's ID
impl From<&PeerId> for uhlc::ID {
    fn from(pid: &PeerId) -> Self {
        uhlc::ID::new(pid.size, pid.id)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Channel {
    BestEffort,
    Reliable,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CongestionControl {
    Block,
    Drop,
}

impl Default for CongestionControl {
    fn default() -> CongestionControl {
        *CONGESTION_CONTROL_DEFAULT
    }
}

impl FromStr for CongestionControl {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "block" => Ok(CongestionControl::Block),
            "drop" => Ok(CongestionControl::Drop),
            _ => {
                let e = format!(
                    "Invalid CongestionControl: {}. Valid values are: 'block' | 'drop'",
                    s
                );
                log::warn!("{}", e);
                zerror!(ZErrorKind::Other { descr: e })
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Reliability {
    BestEffort,
    Reliable,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SubMode {
    Push,
    Pull,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Period {
    pub origin: ZInt,
    pub period: ZInt,
    pub duration: ZInt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubInfo {
    pub reliability: Reliability,
    pub mode: SubMode,
    pub period: Option<Period>,
}

impl Default for SubInfo {
    fn default() -> SubInfo {
        SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        }
    }
}

pub mod queryable {
    pub const ALL_KINDS: super::ZInt = 0x01;
    pub const STORAGE: super::ZInt = 0x02;
    pub const EVAL: super::ZInt = 0x04;
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum ConsolidationMode {
    None,
    Lazy,
    Full,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryConsolidation {
    pub first_routers: ConsolidationMode,
    pub last_router: ConsolidationMode,
    pub reception: ConsolidationMode,
}

impl QueryConsolidation {
    pub fn none() -> Self {
        Self {
            first_routers: ConsolidationMode::None,
            last_router: ConsolidationMode::None,
            reception: ConsolidationMode::None,
        }
    }
}

impl Default for QueryConsolidation {
    fn default() -> Self {
        Self {
            first_routers: ConsolidationMode::Lazy,
            last_router: ConsolidationMode::Lazy,
            reception: ConsolidationMode::Full,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    BestMatching,
    Complete { n: ZInt },
    All,
    None,
}

impl Default for Target {
    fn default() -> Self {
        Target::BestMatching
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryTarget {
    pub kind: ZInt,
    pub target: Target,
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget {
            kind: queryable::ALL_KINDS,
            target: Target::default(),
        }
    }
}
