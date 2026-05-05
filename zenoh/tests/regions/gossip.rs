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

use std::{collections::HashSet, time::Duration};

use futures::{stream::FuturesUnordered, StreamExt};
use zenoh::sample::SampleKind;
use zenoh_config::WhatAmI::{Peer, Router};
use zenoh_core::ztimeout;

use crate::{loc, Node};

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

    async fn wait_for_transports<const N: usize>(
        this: &zenoh::Session,
        others: [&zenoh::Session; N],
    ) {
        let listener = this
            .info()
            .transport_events_listener()
            .history(true)
            .await
            .unwrap();
        let mut stream = listener.stream();

        let mut zids = others.iter().map(|s| s.zid()).collect::<HashSet<_>>();

        while let Some(event) = stream.next().await {
            assert_eq!(event.kind(), SampleKind::Put);

            // Fail if we get an unexpected transport
            assert!(zids.remove(event.transport().zid()));

            if zids.is_empty() {
                break;
            }
        }
    }

    let waits = [
        wait_for_transports(&z9110, [&z9100, &z9111]),
        wait_for_transports(&z9111, [&z9100, &z9110]),
        wait_for_transports(&z9120, [&z9100, &z9121]),
        wait_for_transports(&z9121, [&z9100, &z9120]),
        wait_for_transports(&z9210, [&z9200, &z9211]),
        wait_for_transports(&z9211, [&z9200, &z9210]),
    ]
    .into_iter()
    .collect::<FuturesUnordered<_>>();

    tokio::time::timeout(TIMEOUT, waits.for_each(|_| async {}))
        .await
        .expect("timed out waiting for transports");

    // Verify no unexpected transports exist
    assert_eq!(z9110.info().transports().await.count(), 2);
    assert_eq!(z9111.info().transports().await.count(), 2);
    assert_eq!(z9120.info().transports().await.count(), 2);
    assert_eq!(z9121.info().transports().await.count(), 2);
    assert_eq!(z9210.info().transports().await.count(), 2);
    assert_eq!(z9211.info().transports().await.count(), 2);
}
