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
use zenoh::query::Reply;
use zenoh_core::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(10);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

async fn open_session(listen: &[&str], connect: &[&str]) -> Session {
    let mut config = config::peer();
    config.listen.endpoints = listen
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.connect.endpoints = connect
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening session");
    ztimeout!(zenoh::open(config).res_async()).unwrap()
}

async fn close_session(session: Session) {
    println!("[  ][01d] Closing session");
    ztimeout!(session.close().res_async()).unwrap();
}

#[test]
fn zenoh_events() {
    task::block_on(async {
        zasync_executor_init!();

        let session = open_session(&["tcp/127.0.0.1:18447"], &[]).await;
        let zid = session.zid();
        let sub1 = session
            .declare_subscriber(format!("@/session/{zid}/transport/unicast/*"))
            .res()
            .await
            .unwrap();
        let sub2 = session
            .declare_subscriber(format!("@/session/{zid}/transport/unicast/*/link/*"))
            .res()
            .await
            .unwrap();

        let session2 = open_session(&["tcp/127.0.0.1:18448"], &["tcp/127.0.0.1:18447"]).await;
        let zid2 = session2.zid();

        let sample = ztimeout!(sub1.recv_async());
        assert!(sample.is_ok());
        let key_expr = sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.eq(&format!("@/session/{zid}/transport/unicast/{zid2}")));
        assert!(sample.as_ref().unwrap().kind == SampleKind::Put);

        let sample = ztimeout!(sub2.recv_async());
        assert!(sample.is_ok());
        let key_expr = sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.starts_with(&format!("@/session/{zid}/transport/unicast/{zid2}/link/")));
        assert!(sample.as_ref().unwrap().kind == SampleKind::Put);

        let replies: Vec<Reply> = ztimeout!(session
            .get(format!("@/session/{zid}/transport/unicast/*"))
            .res_async())
        .unwrap()
        .into_iter()
        .collect();
        assert!(replies.len() == 1);
        assert!(replies[0].sample.is_ok());
        let key_expr = replies[0].sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.eq(&format!("@/session/{zid}/transport/unicast/{zid2}")));

        let replies: Vec<Reply> = ztimeout!(session
            .get(format!("@/session/{zid}/transport/unicast/*/link/*"))
            .res_async())
        .unwrap()
        .into_iter()
        .collect();
        assert!(replies.len() == 1);
        assert!(replies[0].sample.is_ok());
        let key_expr = replies[0].sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.starts_with(&format!("@/session/{zid}/transport/unicast/{zid2}/link/")));

        close_session(session2).await;

        let sample = ztimeout!(sub1.recv_async());
        assert!(sample.is_ok());
        let key_expr = sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.eq(&format!("@/session/{zid}/transport/unicast/{zid2}")));
        assert!(sample.as_ref().unwrap().kind == SampleKind::Delete);

        let sample = ztimeout!(sub2.recv_async());
        assert!(sample.is_ok());
        let key_expr = sample.as_ref().unwrap().key_expr.as_str();
        assert!(key_expr.starts_with(&format!("@/session/{zid}/transport/unicast/{zid2}/link/")));
        assert!(sample.as_ref().unwrap().kind == SampleKind::Delete);

        sub2.undeclare().res().await.unwrap();
        sub1.undeclare().res().await.unwrap();
        close_session(session).await;
    });
}
