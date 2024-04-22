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
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_liveliness() {
    let mut c1 = config::peer();
    c1.listen
        .set_endpoints(vec!["tcp/localhost:47447".parse().unwrap()])
        .unwrap();
    c1.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session1 = ztimeout!(zenoh::open(c1).res_async()).unwrap();
    let mut c2 = config::peer();
    c2.connect
        .set_endpoints(vec!["tcp/localhost:47447".parse().unwrap()])
        .unwrap();
    c2.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session2 = ztimeout!(zenoh::open(c2).res_async()).unwrap();

    let sub = ztimeout!(session2
        .liveliness()
        .declare_subscriber("zenoh_liveliness_test")
        .res_async())
    .unwrap();

    let token = ztimeout!(session1
        .liveliness()
        .declare_token("zenoh_liveliness_test")
        .res_async())
    .unwrap();

    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(session2
        .liveliness()
        .get("zenoh_liveliness_test")
        .res_async())
    .unwrap();
    let sample = ztimeout!(replies.recv_async()).unwrap().sample.unwrap();
    assert!(sample.kind == SampleKind::Put);
    assert!(sample.key_expr.as_str() == "zenoh_liveliness_test");

    assert!(ztimeout!(replies.recv_async()).is_err());

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind == SampleKind::Put);
    assert!(sample.key_expr.as_str() == "zenoh_liveliness_test");

    drop(token);

    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(session2
        .liveliness()
        .get("zenoh_liveliness_test")
        .res_async())
    .unwrap();
    assert!(ztimeout!(replies.recv_async()).is_err());

    assert!(replies.try_recv().is_err());
}
