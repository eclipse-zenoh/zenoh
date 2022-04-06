//
// Copyright (c) 2022 ZettaScale Technology
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

//! Properties returned by the [`info`](super::Session::info) function and associated constants.
use zenoh_cfg_properties::{IntKeyProperties, KeyTranscoder};

// Properties returned by info()
pub const ZN_INFO_PID_KEY: u64 = 0x00;
pub const ZN_INFO_PEER_PID_KEY: u64 = 0x01;
pub const ZN_INFO_ROUTER_PID_KEY: u64 = 0x02;

/// A transcoder for [`InfoProperties`](InfoProperties)
/// able to convert string keys to int keys and reverse.
pub struct InfoTranscoder();
impl KeyTranscoder for InfoTranscoder {
    fn encode(key: &str) -> Option<u64> {
        match key {
            "info_pid" => Some(ZN_INFO_PID_KEY),
            "info_peer_pid" => Some(ZN_INFO_PEER_PID_KEY),
            "info_router_pid" => Some(ZN_INFO_ROUTER_PID_KEY),
            _ => None,
        }
    }

    fn decode(key: u64) -> Option<String> {
        match key {
            0x00 => Some("info_pid".to_string()),
            0x01 => Some("info_peer_pid".to_string()),
            0x02 => Some("info_router_pid".to_string()),
            key => Some(key.to_string()),
        }
    }
}

/// A set of Key/Value (`u64`/`String`) pairs returned by [`info`](super::Session::info).
///
/// Multiple values are coma separated.
///
/// The [`IntKeyProperties`](IntKeyProperties) can be converted to (`String`/`String`)
/// [`Properties`](crate::properties::Properties) and reverse.
pub type InfoProperties = IntKeyProperties<InfoTranscoder>;
