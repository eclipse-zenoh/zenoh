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
const RECV_TIMEOUT: Duration = Duration::from_secs(1);

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_matching_status_any() {
    use flume::RecvTimeoutError;

    let session1 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let session2 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let publisher1 = ztimeout!(session1
        .declare_publisher("zenoh_matching_status_any_test")
        .allowed_destination(Locality::Any)
        .res_async())
    .unwrap();

    let matching_listener = ztimeout!(publisher1.matching_listener().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session1
        .declare_subscriber("zenoh_matching_status_any_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(true));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(false));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session2
        .declare_subscriber("zenoh_matching_status_any_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(true));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(false));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_matching_status_remote() {
    use flume::RecvTimeoutError;

    let session1 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let session2 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let publisher1 = ztimeout!(session1
        .declare_publisher("zenoh_matching_status_remote_test")
        .allowed_destination(Locality::Remote)
        .res_async())
    .unwrap();

    let matching_listener = ztimeout!(publisher1.matching_listener().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session1
        .declare_subscriber("zenoh_matching_status_remote_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session2
        .declare_subscriber("zenoh_matching_status_remote_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(true));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(false));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_matching_status_local() {
    use flume::RecvTimeoutError;

    let session1 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let session2 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

    let publisher1 = ztimeout!(session1
        .declare_publisher("zenoh_matching_status_local_test")
        .allowed_destination(Locality::SessionLocal)
        .res_async())
    .unwrap();

    let matching_listener = ztimeout!(publisher1.matching_listener().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session1
        .declare_subscriber("zenoh_matching_status_local_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(true));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.ok().map(|s| s.matching_subscribers()) == Some(false));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    let sub = ztimeout!(session2
        .declare_subscriber("zenoh_matching_status_local_test")
        .res_async())
    .unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());

    ztimeout!(sub.undeclare().res_async()).unwrap();

    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    assert!(received_status.err() == Some(RecvTimeoutError::Timeout));

    let matching_status = ztimeout!(publisher1.matching_status().res_async()).unwrap();
    assert!(!matching_status.matching_subscribers());
}
