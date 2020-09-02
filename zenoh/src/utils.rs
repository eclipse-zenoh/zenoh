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
use crate::net::{encoding, DataInfo, ZInt};
use crate::{ChangeKind, Properties, Timestamp, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn decode_data_info(data_info: Option<DataInfo>) -> (ChangeKind, ZInt, Timestamp) {
    if let Some(info) = data_info {
        (
            info.kind.map_or(ChangeKind::PUT, ChangeKind::from),
            info.encoding.unwrap_or(encoding::APP_OCTET_STREAM),
            info.timestamp.unwrap_or_else(new_reception_timestamp),
        )
    } else {
        // If DataInfo is not present, simulate one,
        // assuming a PUT of a simple buffer,
        // and using a reception timestamp
        (
            ChangeKind::PUT,
            encoding::APP_OCTET_STREAM,
            new_reception_timestamp(),
        )
    }
}

// generate a reception timestamp with id=0x00
fn new_reception_timestamp() -> Timestamp {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Timestamp::new(now.into(), vec![0x00])
}

pub fn properties_to_json_value(props: &Properties) -> Value {
    let json_map = props
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect::<serde_json::map::Map<String, serde_json::Value>>();
    let json_val = serde_json::Value::Object(json_map);
    Value::Json(json_val.to_string())
}
