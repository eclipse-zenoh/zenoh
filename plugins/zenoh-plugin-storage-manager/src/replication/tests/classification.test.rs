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

use std::str::FromStr;

use zenoh::key_expr::OwnedKeyExpr;

use super::*;
use crate::replication::Action;

fn new_event(key_expr: Option<OwnedKeyExpr>, timestamp: Timestamp) -> Event {
    Event::new(key_expr, timestamp, &Action::Put)
}

#[test]
fn test_sub_interval() {
    let hlc = uhlc::HLC::default();

    let event_a = new_event(
        Some(OwnedKeyExpr::from_str("test/a").unwrap()),
        hlc.new_timestamp(),
    );
    let mut sub_interval = SubInterval::default();
    sub_interval.insert_unchecked(event_a.clone());
    assert_eq!(event_a.fingerprint(), sub_interval.fingerprint);

    let event_b = new_event(
        Some(OwnedKeyExpr::from_str("test/b").unwrap()),
        hlc.new_timestamp(),
    );
    sub_interval.insert_unchecked(event_b.clone());

    let event_c = new_event(
        Some(OwnedKeyExpr::from_str("test/c").unwrap()),
        hlc.new_timestamp(),
    );
    sub_interval.insert_unchecked(event_c.clone());
    let mut expected_fingerprint =
        event_a.fingerprint() ^ event_b.fingerprint() ^ event_c.fingerprint();
    assert_eq!(expected_fingerprint, sub_interval.fingerprint);

    let event_d = new_event(
        Some(OwnedKeyExpr::from_str("test/d").unwrap()),
        hlc.new_timestamp(),
    );
    assert_eq!(EventRemoval::NotFound, sub_interval.remove_older(&event_d));
    sub_interval.insert_unchecked(event_d.clone());

    let event_d_new = new_event(event_d.key_expr().clone(), hlc.new_timestamp());
    assert_eq!(
        EventRemoval::RemovedOlder(event_d.clone()),
        sub_interval.remove_older(&event_d_new)
    );
    // NOTE: We added and removed `event_d` the fingerprint should be identical.
    assert_eq!(expected_fingerprint, sub_interval.fingerprint);

    sub_interval.insert_unchecked(event_d_new.clone());
    expected_fingerprint ^= event_d_new.fingerprint();
    assert_eq!(expected_fingerprint, sub_interval.fingerprint);
}

#[test]
fn test_interval() {
    let hlc = uhlc::HLC::default();

    let event_0_0 = new_event(
        Some(OwnedKeyExpr::from_str("test/0/0").unwrap()),
        hlc.new_timestamp(),
    );
    let event_0_1 = new_event(
        Some(OwnedKeyExpr::from_str("test/0/1").unwrap()),
        hlc.new_timestamp(),
    );
    let event_1_0 = new_event(
        Some(OwnedKeyExpr::from_str("test/1/0").unwrap()),
        hlc.new_timestamp(),
    );

    let mut interval = Interval::default();
    interval.insert_unchecked(SubIntervalIdx(0), event_0_0.clone());
    interval.insert_unchecked(SubIntervalIdx(0), event_0_1.clone());
    interval.insert_unchecked(SubIntervalIdx(1), event_1_0.clone());

    let expected_fingerprint =
        event_0_0.fingerprint() ^ event_1_0.fingerprint() ^ event_0_1.fingerprint();
    assert_eq!(expected_fingerprint, interval.fingerprint);

    let event_1_1 = new_event(
        Some(OwnedKeyExpr::from_str("test/1/1").unwrap()),
        hlc.new_timestamp(),
    );
    // No Event with the same key expression.
    assert_eq!(EventRemoval::NotFound, interval.remove_older(&event_1_1));
    // Event already present in the Interval: the event is not the newest.
    interval.insert_unchecked(SubIntervalIdx(1), event_1_1.clone());
    assert_eq!(EventRemoval::KeptNewer, interval.remove_older(&event_1_1));

    let event_1_1_new = new_event(event_1_1.key_expr().clone(), hlc.new_timestamp());
    assert_eq!(
        EventRemoval::RemovedOlder(event_1_1.clone()),
        interval.remove_older(&event_1_1_new)
    );
    // We removed `event_1_1`, we should be back to having only `event_0_0`, `event_0_1` and
    // `event_1_0`.
    assert_eq!(expected_fingerprint, interval.fingerprint);

    // We remove `event_1_0`, there is no event left in SubInterval(1) so it should be removed from
    // the Interval.
    let event_1_0_new = new_event(event_1_0.key_expr().clone(), hlc.new_timestamp());
    assert_eq!(
        EventRemoval::RemovedOlder(event_1_0.clone()),
        interval.remove_older(&event_1_0_new)
    );
    assert!(!interval.sub_intervals.contains_key(&SubIntervalIdx(1)));
}
