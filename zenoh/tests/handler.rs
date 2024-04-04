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
#[test]
fn pubsub_with_ringbuffer() {
    use std::{thread, time::Duration};
    use zenoh::{handlers::RingChannel, prelude::sync::*};

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let sub = zenoh
        .declare_subscriber("test/ringbuffer")
        .with(RingChannel::new(3))
        .res()
        .unwrap();
    for i in 0..10 {
        zenoh
            .put("test/ringbuffer", format!("put{i}"))
            .res()
            .unwrap();
    }
    // Should only receive the last three samples ("put7", "put8", "put9")
    for i in 7..10 {
        assert_eq!(
            sub.recv()
                .unwrap()
                .payload()
                .deserialize::<String>()
                .unwrap(),
            format!("put{i}")
        );
    }
    // Wait for the subscriber to get the value
    thread::sleep(Duration::from_millis(1000));
}

#[test]
fn query_with_ringbuffer() {
    use zenoh::{handlers::RingChannel, prelude::sync::*};

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let queryable = zenoh
        .declare_queryable("test/ringbuffer_query")
        .with(RingChannel::new(1))
        .res()
        .unwrap();

    let _reply1 = zenoh
        .get("test/ringbuffer_query")
        .with_value("query1")
        .res()
        .unwrap();
    let _reply2 = zenoh
        .get("test/ringbuffer_query")
        .with_value("query2")
        .res()
        .unwrap();

    let query = queryable.recv().unwrap();
    // Only receive the latest query
    assert_eq!(
        query
            .value()
            .unwrap()
            .payload
            .deserialize::<String>()
            .unwrap(),
        "query2"
    );
}
