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

use std::{str::FromStr, time::Duration};

use uhlc::HLC;

use super::*;

#[test]
fn test_lower_bounds() {
    let configuration = Configuration::new(
        OwnedKeyExpr::from_str("replication/test/*").unwrap(),
        None,
        ReplicaConfig {
            interval: Duration::from_secs(10),
            sub_intervals: 5,
            hot: 1,
            warm: 5,
            propagation_delay: Duration::from_millis(250),
        },
    );

    assert_eq!(
        IntervalIdx(10),
        configuration.hot_era_lower_bound(IntervalIdx(10))
    );
    assert_eq!(
        IntervalIdx(5),
        configuration.warm_era_lower_bound(IntervalIdx(10))
    );
}

#[test]
fn test_difference() {
    let identical_replica_config = ReplicaConfig {
        interval: Duration::from_secs(10),
        sub_intervals: 5,
        hot: 1,
        warm: 5,
        propagation_delay: Duration::from_millis(250),
    };

    let configuration_a = Configuration::new(
        OwnedKeyExpr::from_str("replication/test/**").unwrap(),
        None,
        identical_replica_config.clone(),
    );

    let configuration_b = Configuration::new(
        OwnedKeyExpr::from_str("replication/test/a/*").unwrap(),
        None,
        identical_replica_config.clone(),
    );

    assert_ne!(configuration_a.fingerprint, configuration_b.fingerprint);

    let configuration_c = Configuration::new(
        configuration_a.storage_key_expr,
        Some(OwnedKeyExpr::from_str("replication/test").unwrap()),
        identical_replica_config,
    );

    assert_ne!(configuration_a.fingerprint, configuration_c.fingerprint);
}

#[test]
fn test_get_classification() {
    let configuration = Configuration::new(
        OwnedKeyExpr::from_str("replication/test/*").unwrap(),
        None,
        ReplicaConfig {
            interval: Duration::from_secs(10),
            sub_intervals: 5,
            hot: 1,
            warm: 5,
            propagation_delay: Duration::from_millis(250),
        },
    );

    let hlc = HLC::default();

    let new_timestamp = |duration: Duration| Timestamp::new(duration.into(), *hlc.get_id());

    // 0 = 0 * 10_000 + 0 * 2_000
    //     ^ interval = 0
    //                  ^ sub-interval = 0
    let timestamp_0_0_lower = new_timestamp(Duration::from_millis(0));
    assert_eq!(
        (IntervalIdx(0), SubIntervalIdx(0)),
        configuration
            .get_time_classification(&timestamp_0_0_lower)
            .unwrap()
    );

    // 1_999 = 0 * 10_000 + 0 * 2_000 + 1999
    //         ^ interval = 0
    //                      ^ sub-interval = 0
    let timestamp_0_0_upper = new_timestamp(Duration::from_millis(1999));
    assert_eq!(
        (IntervalIdx(0), SubIntervalIdx(0)),
        configuration
            .get_time_classification(&timestamp_0_0_upper)
            .unwrap()
    );

    // 2_000 = 0 * 10_000 + 1 * 2_000 + 0
    //         ^ interval = 0
    //                      ^ sub-interval = 1
    let timestamp_0_1_lower = new_timestamp(Duration::from_millis(2_000));
    assert_eq!(
        (IntervalIdx(0), SubIntervalIdx(1)),
        configuration
            .get_time_classification(&timestamp_0_1_lower)
            .unwrap()
    );

    // 19_999 = 1 * 10_000 + 4 * 2_000 + 1_999
    //          ^ interval = 1
    //                       ^ sub-interval = 4
    let timestamp_1_4_upper = new_timestamp(Duration::from_millis(19_999));
    assert_eq!(
        (IntervalIdx(1), SubIntervalIdx(4)),
        configuration
            .get_time_classification(&timestamp_1_4_upper)
            .unwrap()
    );

    // 106_042 = 10 * 10_000 + 3 * 2_000 + 42
    //           ^^ interval = 10
    //                         ^ sub-interval = 3
    let timestamp_10_3 = new_timestamp(Duration::from_millis(106_042));
    assert_eq!(
        (IntervalIdx(10), SubIntervalIdx(3)),
        configuration
            .get_time_classification(&timestamp_10_3)
            .unwrap()
    );
}
