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

#![cfg(feature = "internal_config")]

use std::time::Duration;

use zenoh::{query::Reply, sample::SampleKind, Session};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(10);

async fn open_session(listen: &[&str], connect: &[&str]) -> Session {
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(
            listen
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config
        .connect
        .endpoints
        .set(
            connect
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening session");
    ztimeout!(zenoh::open(config)).unwrap()
}

async fn close_session(session: Session) {
    println!("[  ][01d] Closing session");
    ztimeout!(session.close()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_events() {
    let session = open_session(&["tcp/127.0.0.1:18447"], &[]).await;
    let zid = session.zid();
    let sub1 =
        ztimeout!(session.declare_subscriber(format!("@/{zid}/session/transport/unicast/*")))
            .unwrap();
    let sub2 = ztimeout!(
        session.declare_subscriber(format!("@/{zid}/session/transport/unicast/*/link/*"))
    )
    .unwrap();

    let session2 = open_session(&["tcp/127.0.0.1:18448"], &["tcp/127.0.0.1:18447"]).await;
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

    close_session(session2).await;

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
    close_session(session).await;
}
