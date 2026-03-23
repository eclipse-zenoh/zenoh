//
// Copyright (c) 2026 ZettaScale Technology
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

use zenoh_config::WhatAmI::{Peer, Router};
use zenoh_core::ztimeout;

use crate::{loc, skip_fmt, Node};

const TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn regions_gossip() {
    // Scenario
    //     R        R
    // P P   P P   P P

    let z9100 = ztimeout!(Node::new(Router, "60551d9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]}
        ]}"
        )
        .open());

    let z9200 = ztimeout!(Node::new(Router, "60551d9200")
        .listen("tcp/0.0.0.0:0")
        .connect(&[loc!(z9100)])
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "60551d9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .open());
    let z9111 = ztimeout!(Node::new(Peer, "60551d9111")
        .region("region1")
        .connect(&[loc!(z9100)])
        .open());

    let z9120 = ztimeout!(Node::new(Peer, "60551d9120")
        .region("region2")
        .connect(&[loc!(z9100)])
        .open());
    let z9121 = ztimeout!(Node::new(Peer, "60551d9121")
        .region("region2")
        .connect(&[loc!(z9100)])
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "60551d9210")
        .region("region3")
        .connect(&[loc!(z9200)])
        .open());
    let z9211 = ztimeout!(Node::new(Peer, "60551d9211")
        .region("region3")
        .connect(&[loc!(z9200)])
        .open());

    skip_fmt! {
        assert!(z9110.info().transports().await.count() == 2);
        assert!(z9110.info().transports().await.any(|t| *t.zid() == z9100.zid()));
        assert!(z9110.info().transports().await.any(|t| *t.zid() == z9111.zid()));
        assert!(z9111.info().transports().await.count() == 2);
        assert!(z9111.info().transports().await.any(|t| *t.zid() == z9100.zid()));
        assert!(z9111.info().transports().await.any(|t| *t.zid() == z9110.zid()));

        assert!(z9120.info().transports().await.count() == 2);
        assert!(z9120.info().transports().await.any(|t| *t.zid() == z9100.zid()));
        assert!(z9120.info().transports().await.any(|t| *t.zid() == z9121.zid()));
        assert!(z9121.info().transports().await.count() == 2);
        assert!(z9121.info().transports().await.any(|t| *t.zid() == z9100.zid()));
        assert!(z9121.info().transports().await.any(|t| *t.zid() == z9120.zid()));

        assert!(z9210.info().transports().await.count() == 2);
        assert!(z9210.info().transports().await.any(|t| *t.zid() == z9200.zid()));
        assert!(z9210.info().transports().await.any(|t| *t.zid() == z9211.zid()));
        assert!(z9211.info().transports().await.count() == 2);
        assert!(z9211.info().transports().await.any(|t| *t.zid() == z9200.zid()));
        assert!(z9211.info().transports().await.any(|t| *t.zid() == z9210.zid()));
    }
}
