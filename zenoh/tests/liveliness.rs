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
use async_std::prelude::FutureExt;
use async_std::task;
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh_core::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

#[cfg(feature = "unstable")]
#[test]
fn zenoh_liveliness() {
    task::block_on(async {
        zasync_executor_init!();

        let session1 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

        let session2 = ztimeout!(zenoh::open(config::peer()).res_async()).unwrap();

        let replies = ztimeout!(session2
            .liveliness()
            .get("zenoh_liveliness_test")
            .res_async())
        .unwrap();
        assert!(replies.into_iter().count() == 0);

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

        task::sleep(SLEEP).await;

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

        task::sleep(SLEEP).await;

        let replies = ztimeout!(session2
            .liveliness()
            .get("zenoh_liveliness_test")
            .res_async())
        .unwrap();
        assert!(ztimeout!(replies.recv_async()).is_err());

        assert!(replies.try_recv().is_err());
    });
}
