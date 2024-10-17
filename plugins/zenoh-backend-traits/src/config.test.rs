//
// Copyright (c) 2024 ZettaScale Technology
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

use std::time::Duration;

use serde_json::json;

use super::StorageConfig;
use crate::config::ReplicaConfig;

#[test]
fn test_replica_config() {
    let empty_config = json!({
        "key_expr": "test/**",
        "volume": "memory",
        "replication": {}
    });
    let storage_config =
        StorageConfig::try_from("test-plugin", "test-storage", &empty_config).unwrap();
    assert_eq!(storage_config.replication, Some(ReplicaConfig::default()));

    let incorrect_propagation_delay_config = json!({
        "key_expr": "test/**",
        "volume": "memory",
        "replication": {
            "interval": 1,
            "propagation_delay": 750,
        }
    });
    let result = StorageConfig::try_from(
        "test-plugin",
        "test-storage",
        &incorrect_propagation_delay_config,
    );
    let err = result.unwrap_err();
    let expected_error_msg =
        "consider increasing the `interval` to at least twice its value (i.e. 1.5)";
    assert!(
        err.to_string().contains(expected_error_msg),
        "\nExpected to contain: {expected_error_msg}
Actual message: {err}",
    );

    let replica_config = json!({
        "key_expr": "test/**",
        "volume": "memory",
        "replication": {
            "interval": 10,
            "sub_intervals": 4,
            "hot": 6,
            "warm": 60,
            "propagation_delay": 250,
        }
    });
    let storage_config =
        StorageConfig::try_from("test-plugin", "test-storage", &replica_config).unwrap();
    assert_eq!(
        storage_config.replication,
        Some(ReplicaConfig {
            interval: Duration::from_secs(10),
            sub_intervals: 4,
            hot: 6,
            warm: 60,
            propagation_delay: Duration::from_millis(250)
        })
    );
}
