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

//! Constants and helpers to build the configuration [Properties](super::Properties)
//! to pass to [Zenoh::new](super::Zenoh::new).
//!
//! Configuration [Properties](Properties) are a map of string key/value pairs.
//! Multiple values for a same key are coma separated.
//!
//! Accepted configuration properties :
//!
//! * `"mode"` - The library mode.
//!     * Accepted values : `"peer"`, `"client"`.
//!     * Default value : `"peer"`.
//!
//! * `"peer"` - The locator of a peer to connect to.
//!     * Accepted values : locators (ex: `"tcp/10.10.10.10:7447"`).
//!     * Default value : None.
//!     * Multiple coma separated values accepted (ex: `"tcp/10.10.10.10:7447,tcp/11.11.11.11:7447"`).
//!
//! * `"listener"` - A locator to listen on.
//!     * Accepted values : locators (ex: `"tcp/10.10.10.10:7447"`).
//!     * Default value : None.
//!     * Multiple coma separated values accepted (ex: `"tcp/10.10.10.10:7447,tcp/11.11.11.11:7447"`).
//!
//! * `"user"` - The user name to use for authentication.
//!     * Accepted values : string.
//!     * Default value : None.
//!
//! * `"password"` - The password to use for authentication.
//!     * Accepted values : string.
//!     * Default value : None.
//!
//! * `"multicast_interface"` - The network interface to use for multicast discovery.
//!     * Accepted values : `"auto"`, ip address, interface name.
//!     * Default value : `"auto"`.
//!
//! * `"scouting_delay"` - In peer mode, the period dedicated to scouting first remote peers before doing anything else.
//!     * Accepted values : float in seconds.
//!     * Default value : `"0.2"`.
//!
//! * `"add_timestamp"` - Indicates if data messages should be timestamped.
//!     * Accepted values : `"true"`, `"false"`.
//!     * Default value : `"false"`.
//!
//! * `"local_routing"` - Indicates if local writes/queries should reach local subscribers/queryables.
//!     * Accepted values : `"true"`, `"false"`.
//!     * Default value : `"true"`.

use crate::net::config::*;
use crate::Properties;
use std::collections::HashMap;

pub fn empty() -> Properties {
    Properties(HashMap::new())
}

pub fn default() -> Properties {
    peer()
}

pub fn peer() -> Properties {
    let mut config = empty();
    config.insert("mode".to_string(), "peer".to_string());
    config
}

pub fn client() -> Properties {
    let mut config = empty();
    config.insert("mode".to_string(), "peer".to_string());
    config
}

fn str_key_to_zn_key(key: &str) -> Option<zenoh_protocol::core::ZInt> {
    match &key.to_lowercase()[..] {
        "mode" => Some(ZN_MODE_KEY),
        "peer" => Some(ZN_PEER_KEY),
        "listener" => Some(ZN_LISTENER_KEY),
        "user" => Some(ZN_USER_KEY),
        "password" => Some(ZN_PASSWORD_KEY),
        "multicast_interface" => Some(ZN_MULTICAST_INTERFACE_KEY),
        "scouting_delay" => Some(ZN_SCOUTING_DELAY_KEY),
        "add_timestamp" => Some(ZN_ADD_TIMESTAMP_KEY),
        "local_routing" => Some(ZN_LOCAL_ROUTING_KEY),
        _ => None,
    }
}

impl Into<crate::net::Properties> for Properties {
    fn into(self) -> crate::net::Properties {
        let mut zn_props = vec![];
        for (k, v) in self.0.iter() {
            if let Some(k) = str_key_to_zn_key(k) {
                for v in v.split(',') {
                    zn_props.push((k, v.as_bytes().to_vec()));
                }
            }
        }
        zn_props
    }
}
