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

//! Some useful operations for the zenoh API.

use crate::{Properties, Timestamp, TimestampId, Value};

/// Generates a reception [`Timestamp`] with id=0x00.  
/// This operation should be called if a timestamp is required for an incoming [`zenoh::net::Sample`](crate::net::Sample)
/// that doesn't contain any data_info or timestamp within its data_info.
pub fn new_reception_timestamp() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Timestamp::new(
        now.into(),
        TimestampId::new(1, [0u8; TimestampId::MAX_SIZE]),
    )
}

/// Convert a set of [`Properties`] into a [`Value::Json`].  
/// For instance such Properties: `[("k1", "v1"), ("k2, v2")]`  
/// are converted into such Json: `{ "k1": "v1", "k2": "v2" }`
pub fn properties_to_json_value(props: &Properties) -> Value {
    let json_map = props
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect::<serde_json::map::Map<String, serde_json::Value>>();
    let json_val = serde_json::Value::Object(json_map);
    Value::Json(json_val.to_string())
}
