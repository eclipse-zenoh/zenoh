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

//! Properties to pass to [open](super::open) and [scout](super::scout) functions as configuration
//! and associated constants.
pub use zenoh_util::properties::config::*;

/// A set of Key/Value (`u64`/`String`) pairs to pass to [open](super::open)  
/// to configure the zenoh-net [Session](super::super::Session).
///
/// Multiple values are coma separated.
///
/// The [IntKeyProperties](IntKeyProperties) can be built from (`String`/`String`)
/// [Properties](super::super::Properties) and reverse.

/// Creates an empty zenoh net Session configuration.
pub fn empty() -> ConfigProperties {
    ConfigProperties::default()
}

/// Creates a default zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "peer")`
pub fn default() -> ConfigProperties {
    peer()
}

/// Creates a default `'peer'` mode zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "peer")`
pub fn peer() -> ConfigProperties {
    let mut props = ConfigProperties::default();
    props.insert(ZN_MODE_KEY, "peer".to_string());
    props
}

/// Creates a default `'client'` mode zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "client")`
///
/// If the given peer locator is not `None`, the returned configuration also contains :
///  - `(ZN_PEER_KEY, <peer>)`
pub fn client(peer: Option<String>) -> ConfigProperties {
    let mut props = ConfigProperties::default();
    props.insert(ZN_MODE_KEY, "client".to_string());
    if let Some(peer) = peer {
        props.insert(ZN_PEER_KEY, peer);
    }
    props
}
