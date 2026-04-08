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
#![cfg(feature = "unstable")]
mod common;
use std::time::Duration;

use zenoh::{query::Reply, sample::SampleKind};
use zenoh_core::ztimeout;

use crate::common::TestSessions;

const TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_events() {
    let mut test_context = TestSessions::new();

    let session = test_context.open_listener().await;
    let zid = session.zid();
    let sub1 =
        ztimeout!(session.declare_subscriber(format!("@/{zid}/session/transport/unicast/*")))
            .unwrap();
    let sub2 = ztimeout!(
        session.declare_subscriber(format!("@/{zid}/session/transport/unicast/*/link/*"))
    )
    .unwrap();

    let session2 = test_context.open_connector().await;
    let zid2 = session2.zid();

    let sample = ztimeout!(sub1.recv_async());
    assert!(sample.is_ok());
    let key_expr = sample.as_ref().unwrap().key_expr().as_str();
    assert!(key_expr.eq(&format!("@/{zid}/session/transport/unicast/{zid2}")));
    assert!(sample.as_ref().unwrap().kind() == SampleKind::Put);

    let sample = ztimeout!(sub2.recv_async());
    assert!(sample.is_ok());
    let key_expr = sample.as_ref().unwrap().key_expr().as_str();
    assert!(key_expr.starts_with(&format!("@/{zid}/session/transport/unicast/{zid2}/link/")));
    assert!(sample.as_ref().unwrap().kind() == SampleKind::Put);

    let replies: Vec<Reply> =
        ztimeout!(session.get(format!("@/{zid}/session/transport/unicast/*")))
            .unwrap()
            .into_iter()
            .collect();
    assert!(replies.len() == 1);
    assert!(replies[0].result().is_ok());
    let key_expr = replies[0].result().unwrap().key_expr().as_str();
    assert!(key_expr.eq(&format!("@/{zid}/session/transport/unicast/{zid2}")));

    let replies: Vec<Reply> =
        ztimeout!(session.get(format!("@/{zid}/session/transport/unicast/*/link/*")))
            .unwrap()
            .into_iter()
            .collect();
    assert!(replies.len() == 1);
    assert!(replies[0].result().is_ok());
    let key_expr = replies[0].result().unwrap().key_expr().as_str();
    assert!(key_expr.starts_with(&format!("@/{zid}/session/transport/unicast/{zid2}/link/")));

    ztimeout!(session2.close()).unwrap();

    let sample = ztimeout!(sub1.recv_async());
    assert!(sample.is_ok());
    let key_expr = sample.as_ref().unwrap().key_expr().as_str();
    assert!(key_expr.eq(&format!("@/{zid}/session/transport/unicast/{zid2}")));
    assert!(sample.as_ref().unwrap().kind() == SampleKind::Delete);

    let sample = ztimeout!(sub2.recv_async());
    assert!(sample.is_ok());
    let key_expr = sample.as_ref().unwrap().key_expr().as_str();
    assert!(key_expr.starts_with(&format!("@/{zid}/session/transport/unicast/{zid2}/link/")));
    assert!(sample.as_ref().unwrap().kind() == SampleKind::Delete);

    ztimeout!(sub2.undeclare()).unwrap();
    ztimeout!(sub1.undeclare()).unwrap();
    ztimeout!(session.close()).unwrap();
}
