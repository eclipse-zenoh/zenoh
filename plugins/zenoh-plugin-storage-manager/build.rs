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
use zenoh_backend_traits::config::PluginConfig;

fn main() {
    // Add rustc version to zenohd
    let version_meta = rustc_version::version_meta().unwrap();
    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        version_meta.short_version_string
    );
    // Generate default config schema
    let schema = schema_for!(PluginConfig);
    let schema_file =
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("config_schema.json5");
    std::fs::write(&schema_file, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

    // Check that the example config matches the schema
    let schema = std::fs::read_to_string(schema_file).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema).unwrap();
    let schema = jsonschema::JSONSchema::compile(&schema).unwrap();
    let config = std::fs::read_to_string("config.json5").unwrap();
    let config: serde_json::Value = serde_json::from_str(&config).unwrap();
    if let Err(es) = schema.validate(&config) {
        let es = es.map(|e| format!("{}", e)).collect::<Vec<_>>().join("\n");
        panic!("config.json5 schema validation error: {}", es);
    };
}
