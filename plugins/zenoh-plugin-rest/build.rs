//
// Copyright (c) 2023 ZettaScale Technology
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
use schemars::schema_for;

use crate::config::Config;

#[path = "src/config.rs"]
mod config;

fn main() {
    // Add rustc version to zenohd
    let version_meta = rustc_version::version_meta().unwrap();
    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        version_meta.short_version_string
    );

    let schema = serde_json::to_value(schema_for!(Config)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let config = std::fs::read_to_string("config.json5").unwrap();
    let config: serde_json::Value = serde_json::from_str(&config).unwrap();
    if let Err(es) = validator.validate(&config) {
        let es = es.map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
        panic!("config.json5 schema validation error: {es}");
    };
}
