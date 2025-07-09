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
use zenoh::{config::Config, Wait};

#[test]
fn attachment_pubsub() {
    let zenoh = zenoh::open(Config::default()).wait().unwrap();
    const ATTACHMENT: &[u8] = b"pubsub attachment";
    zenoh
        .declare_subscriber("test/attachment")
        .callback(|sample| {
            println!("{}", sample.payload().try_to_string().unwrap());
            assert_eq!(sample.attachment().unwrap().to_bytes(), ATTACHMENT);
        })
        .wait()
        .unwrap();
    let publisher = zenoh.declare_publisher("test/attachment").wait().unwrap();
    for _ in 0..10 {
        zenoh
            .put("test/attachment", "put")
            .attachment(ATTACHMENT)
            .wait()
            .unwrap();
        publisher
            .put("publisher")
            .attachment(ATTACHMENT)
            .wait()
            .unwrap();
    }
}

#[test]
fn attachment_queries() {
    let zenoh = zenoh::open(Config::default()).wait().unwrap();
    const QUERY_ATTACHMENT: &[u8] = b"query attachment";
    const REPLY_ATTACHMENT: &[u8] = b"reply attachment";
    let _queryable = zenoh
        .declare_queryable("test/attachment")
        .callback(|query| {
            println!("{}", query.payload().unwrap().try_to_string().unwrap());
            assert_eq!(query.attachment().unwrap().to_bytes(), QUERY_ATTACHMENT);
            query
                .reply(query.key_expr().clone(), query.payload().unwrap().clone())
                .attachment(REPLY_ATTACHMENT)
                .wait()
                .unwrap();
        })
        .wait()
        .unwrap();
    for _ in 0..10 {
        let get = zenoh
            .get("test/attachment")
            .payload("query")
            .attachment(QUERY_ATTACHMENT)
            .wait()
            .unwrap();
        while let Ok(reply) = get.recv() {
            let response = reply.result().unwrap();
            assert_eq!(response.attachment().unwrap().to_bytes(), REPLY_ATTACHMENT);
        }
    }
}
