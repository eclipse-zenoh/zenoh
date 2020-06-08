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
use std::convert::From;
use std::fmt;
use std::sync::atomic::AtomicU64;
use uuid::Uuid;

pub mod rname;

pub type ZInt = u64;
pub type AtomicZInt = AtomicU64;
pub const ZINT_MAX_BYTES : usize = 10;

pub type ResourceId = ZInt;
pub const NO_RESOURCE_ID: ResourceId = 0;

///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~      id       — if ResName{name} : id=0
/// +-+-+-+-+-+-+-+-+
/// ~  name/suffix  ~ if flag C!=1 in Message's header
/// +---------------+
///
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum ResKey {
  RName(String),
  RId(ResourceId),
  RIdWithSuffix(ResourceId, String),
}
use ResKey::*;

impl ResKey {
  pub fn rid(&self) -> ResourceId {
      match self {
          RName(_) => NO_RESOURCE_ID,
          RId(rid) | RIdWithSuffix(rid, _) => *rid,
      }
  }

  pub fn is_numerical(&self) -> bool {
    match self {
      RId(_) => true,
      _ => false
    }
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
  fn from(rid: ResourceId) -> ResKey {
      RId(rid)
  }
}

impl From<&str> for ResKey {
  fn from(name: &str) -> ResKey {
      RName(name.to_string())
  }
}

impl From<String> for ResKey {
  fn from(name: String) -> ResKey {
      RName(name)
  }
}

impl From<(ResourceId, &str)> for ResKey {
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
  fn from(key: &'a ResKey) -> (ResourceId, &'a str) {
    match key {
      RId(rid) => (*rid, ""),
      RName(name) => (NO_RESOURCE_ID, &name[..]), //(&(0 as u64)
      RIdWithSuffix(rid, suffix) => (*rid, &suffix[..])
    }
  }
}


#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key:   ZInt,
    pub value: Vec<u8>
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PeerId {
    pub id: Vec<u8>
}

impl fmt::Debug for PeerId {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{}", hex::encode_upper(&self.id))
  }
}

impl fmt::Display for PeerId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(self, f)
  }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimeStamp {
  pub time: u64,
  pub id: Uuid
}

impl fmt::Debug for TimeStamp {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}/{}", self.time, self.id)
  }
}

impl fmt::Display for TimeStamp {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(self, f)
  }
}
