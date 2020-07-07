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
use zenoh_protocol::core;
use log::debug;

mod types;
pub use types::*;

mod consts;
pub use consts::*;

#[macro_use]
mod session;
pub use session::*;

pub use zenoh_protocol::proto::encoding;
pub use zenoh_protocol::proto::kind;

pub mod queryable { pub use zenoh_protocol::core::queryable::*; }

pub const LOCATOR_AUTO: &str = "auto";

pub fn rname_intersect(s1: &str, s2: &str) -> bool {
    core::rname::intersect(s1, s2)
}

pub async fn scout(_iface: &str, _tries: usize, _period: usize) -> Vec<String> {
    // @TODO: implement
    debug!("scout({}, {}, {})", _iface, _tries, _period);
    vec![]
}

/// # Example:
/// ```
/// use zenoh::net::*;
///
/// let session = open(Config::peer(), None);
/// 
/// ```
pub async fn open(config: Config, ps: Option<Properties>) -> ZResult<Session> {
    debug!("open(\"{}\", {:?})", config, ps);
    Ok(Session::new(config, ps).await)
}

