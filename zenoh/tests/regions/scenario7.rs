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

// Scenario 7
//  R R    R R    R R
// P P P  P P P  P P P

use std::time::Duration;

use futures::StreamExt;
use predicates::Predicate;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    sample::SampleKind,
    Wait,
};
use zenoh_config::WhatAmI::{Peer, Router};
use zenoh_core::{lazy_static, ztimeout};

use crate::{loc, predicates_ext, skip_fmt, unbounded_sink, Node};

const TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    static ref STORAGE: tracing_capture::SharedStorage = tracing_capture::SharedStorage::default();
}

fn init_tracing_subscriber() {
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("debug,zenoh::net::routing::dispatcher=trace")
        .finish();
    let subscriber = subscriber.with(tracing_capture::CaptureLayer::new(&STORAGE));
    tracing::subscriber::set_global_default(subscriber).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "71aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.1.1:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "71aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "71aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.1:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "71aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.1.1:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "71aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.1:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "71aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.1:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "71aa9110")
        .multicast("224.7.1.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "71aa9120")
        .multicast("224.7.1.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "71aa9130")
        .multicast("224.7.1.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "71aa9210")
        .multicast("224.7.1.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "71aa9220")
        .multicast("224.7.1.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "71aa9230")
        .multicast("224.7.1.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "71aa9310")
        .multicast("224.7.1.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "71aa9320")
        .multicast("224.7.1.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "71aa9330")
        .multicast("224.7.1.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            z9110.put("test", "9110").await.unwrap();
            z9120.put("test", "9120").await.unwrap();
            z9130.put("test", "9130").await.unwrap();
            z9210.put("test", "9210").await.unwrap();
            z9220.put("test", "9220").await.unwrap();
            z9230.put("test", "9230").await.unwrap();
            z9310.put("test", "9310").await.unwrap();
            z9320.put("test", "9320").await.unwrap();
            z9330.put("test", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "71aa9110", "71aa9120", "71aa9130", "71aa9210", "71aa9220", "71aa9230", "71aa9310",
        "71aa9320", "71aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "71ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.1.2:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "71ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "71ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.2:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "71ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.1.2:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "71ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.2:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "71ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.2:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "71ab9110")
        .multicast("224.7.1.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "71ab9120")
        .multicast("224.7.1.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "71ab9130")
        .multicast("224.7.1.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "71ab9210")
        .multicast("224.7.1.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "71ab9220")
        .multicast("224.7.1.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "71ab9230")
        .multicast("224.7.1.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "71ab9310")
        .multicast("224.7.1.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "71ab9320")
        .multicast("224.7.1.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "71ab9330")
        .multicast("224.7.1.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9110 = z9110.declare_publisher("test").await.unwrap();
    let p9120 = z9120.declare_publisher("test").await.unwrap();
    let p9130 = z9130.declare_publisher("test").await.unwrap();
    let p9210 = z9210.declare_publisher("test").await.unwrap();
    let p9220 = z9220.declare_publisher("test").await.unwrap();
    let p9230 = z9230.declare_publisher("test").await.unwrap();
    let p9310 = z9310.declare_publisher("test").await.unwrap();
    let p9320 = z9320.declare_publisher("test").await.unwrap();
    let p9330 = z9330.declare_publisher("test").await.unwrap();

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            p9110.put("9110").await.unwrap();
            p9120.put("9120").await.unwrap();
            p9130.put("9130").await.unwrap();
            p9210.put("9210").await.unwrap();
            p9220.put("9220").await.unwrap();
            p9230.put("9230").await.unwrap();
            p9310.put("9310").await.unwrap();
            p9320.put("9320").await.unwrap();
            p9330.put("9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "71ab9110", "71ab9120", "71ab9130", "71ab9210", "71ab9220", "71ab9230", "71ab9310",
        "71ab9320", "71ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "71ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.1.3:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "71ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "71ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.3:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "71ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.1.3:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "71ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.3:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "71ac9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.3:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "71ac9110")
        .multicast("224.7.1.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "71ac9120")
        .multicast("224.7.1.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "71ac9130")
        .multicast("224.7.1.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "71ac9210")
        .multicast("224.7.1.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "71ac9220")
        .multicast("224.7.1.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "71ac9230")
        .multicast("224.7.1.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "71ac9310")
        .multicast("224.7.1.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "71ac9320")
        .multicast("224.7.1.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "71ac9330")
        .multicast("224.7.1.3:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    skip_fmt! {ztimeout!(async {
        loop {
            if z9110.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9120.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9130.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9210.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9220.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9230.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9310.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9320.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9330.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })};

    let s = STORAGE.lock();

    for zid in [
        "71ac9110", "71ac9120", "71ac9130", "71ac9210", "71ac9220", "71ac9230", "71ac9310",
        "71ac9320", "71ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "71ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.1.4:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "71ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "71ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.4:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "71ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.1.4:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "71ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.4:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "71ad9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.4:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "71ad9110")
        .multicast("224.7.1.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "71ad9120")
        .multicast("224.7.1.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "71ad9130")
        .multicast("224.7.1.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "71ad9210")
        .multicast("224.7.1.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "71ad9220")
        .multicast("224.7.1.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "71ad9230")
        .multicast("224.7.1.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "71ad9310")
        .multicast("224.7.1.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "71ad9320")
        .multicast("224.7.1.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "71ad9330")
        .multicast("224.7.1.4:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();

        let q9110 = z9110.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9120 = z9120.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9130 = z9130.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9210 = z9210.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9220 = z9220.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9230 = z9230.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9310 = z9310.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9320 = z9320.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9330 = z9330.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            if q9110.get().await.unwrap().iter().count() == 9
                && q9120.get().await.unwrap().iter().count() == 9
                && q9130.get().await.unwrap().iter().count() == 9
                && q9210.get().await.unwrap().iter().count() == 9
                && q9220.get().await.unwrap().iter().count() == 9
                && q9230.get().await.unwrap().iter().count() == 9
                && q9310.get().await.unwrap().iter().count() == 9
                && q9320.get().await.unwrap().iter().count() == 9
                && q9330.get().await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "71ad9110", "71ad9120", "71ad9130", "71ad9210", "71ad9220", "71ad9230", "71ad9310",
        "71ad9320", "71ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order1_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "71ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.1.5:9100")
        .open());
    let _z9101 = ztimeout!(Node::new(Router, "71ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.5:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "71ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.1.5:9200")
        .open());
    let _z9201 = ztimeout!(Node::new(Router, "71ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.1.5:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "71ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.5:9300")
        .open());
    let _z9301 = ztimeout!(Node::new(Router, "71ae9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.1.5:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "71ae9110")
        .multicast("224.7.1.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "71ae9120")
        .multicast("224.7.1.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "71ae9130")
        .multicast("224.7.1.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "71ae9210")
        .multicast("224.7.1.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "71ae9220")
        .multicast("224.7.1.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "71ae9230")
        .multicast("224.7.1.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "71ae9310")
        .multicast("224.7.1.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "71ae9320")
        .multicast("224.7.1.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "71ae9330")
        .multicast("224.7.1.5:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let t9110 = z9110.liveliness().declare_token("test/9110").await.unwrap();
    let t9120 = z9120.liveliness().declare_token("test/9120").await.unwrap();
    let t9130 = z9130.liveliness().declare_token("test/9130").await.unwrap();
    let t9210 = z9210.liveliness().declare_token("test/9210").await.unwrap();
    let t9220 = z9220.liveliness().declare_token("test/9220").await.unwrap();
    let t9230 = z9230.liveliness().declare_token("test/9230").await.unwrap();
    let t9310 = z9310.liveliness().declare_token("test/9310").await.unwrap();
    let t9320 = z9320.liveliness().declare_token("test/9320").await.unwrap();
    let t9330 = z9330.liveliness().declare_token("test/9330").await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    t9110.undeclare().await.unwrap();
    t9120.undeclare().await.unwrap();
    t9130.undeclare().await.unwrap();
    t9210.undeclare().await.unwrap();
    t9220.undeclare().await.unwrap();
    t9230.undeclare().await.unwrap();
    t9310.undeclare().await.unwrap();
    t9320.undeclare().await.unwrap();
    t9330.undeclare().await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Delete) == 9)
            {
                break;
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "72aa9110")
        .multicast("224.7.2.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "72aa9120")
        .multicast("224.7.2.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "72aa9130")
        .multicast("224.7.2.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "72aa9210")
        .multicast("224.7.2.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "72aa9220")
        .multicast("224.7.2.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "72aa9230")
        .multicast("224.7.2.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "72aa9310")
        .multicast("224.7.2.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "72aa9320")
        .multicast("224.7.2.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "72aa9330")
        .multicast("224.7.2.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let z9100 = ztimeout!(Node::new(Router, "72aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.2.1:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "72aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "72aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.1:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "72aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.2.1:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "72aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.1:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "72aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.1:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            z9110.put("test", "9110").await.unwrap();
            z9120.put("test", "9120").await.unwrap();
            z9130.put("test", "9130").await.unwrap();
            z9210.put("test", "9210").await.unwrap();
            z9220.put("test", "9220").await.unwrap();
            z9230.put("test", "9230").await.unwrap();
            z9310.put("test", "9310").await.unwrap();
            z9320.put("test", "9320").await.unwrap();
            z9330.put("test", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "72aa9110", "72aa9120", "72aa9130", "72aa9210", "72aa9220", "72aa9230", "72aa9310",
        "72aa9320", "72aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "72ab9110")
        .multicast("224.7.2.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "72ab9120")
        .multicast("224.7.2.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "72ab9130")
        .multicast("224.7.2.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "72ab9210")
        .multicast("224.7.2.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "72ab9220")
        .multicast("224.7.2.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "72ab9230")
        .multicast("224.7.2.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "72ab9310")
        .multicast("224.7.2.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "72ab9320")
        .multicast("224.7.2.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "72ab9330")
        .multicast("224.7.2.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9110 = z9110.declare_publisher("test").await.unwrap();
    let p9120 = z9120.declare_publisher("test").await.unwrap();
    let p9130 = z9130.declare_publisher("test").await.unwrap();
    let p9210 = z9210.declare_publisher("test").await.unwrap();
    let p9220 = z9220.declare_publisher("test").await.unwrap();
    let p9230 = z9230.declare_publisher("test").await.unwrap();
    let p9310 = z9310.declare_publisher("test").await.unwrap();
    let p9320 = z9320.declare_publisher("test").await.unwrap();
    let p9330 = z9330.declare_publisher("test").await.unwrap();

    let z9100 = ztimeout!(Node::new(Router, "72ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.2.2:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "72ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "72ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.2:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "72ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.2.2:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "72ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.2:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "72ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.2:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            p9110.put("9110").await.unwrap();
            p9120.put("9120").await.unwrap();
            p9130.put("9130").await.unwrap();
            p9210.put("9210").await.unwrap();
            p9220.put("9220").await.unwrap();
            p9230.put("9230").await.unwrap();
            p9310.put("9310").await.unwrap();
            p9320.put("9320").await.unwrap();
            p9330.put("9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "72ab9110", "72ab9120", "72ab9130", "72ab9210", "72ab9220", "72ab9230", "72ab9310",
        "72ab9320", "72ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "72ac9110")
        .multicast("224.7.2.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "72ac9120")
        .multicast("224.7.2.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "72ac9130")
        .multicast("224.7.2.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "72ac9210")
        .multicast("224.7.2.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "72ac9220")
        .multicast("224.7.2.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "72ac9230")
        .multicast("224.7.2.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "72ac9310")
        .multicast("224.7.2.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "72ac9320")
        .multicast("224.7.2.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "72ac9330")
        .multicast("224.7.2.3:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();
    }

    let z9100 = ztimeout!(Node::new(Router, "72ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.2.3:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "72ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "72ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.3:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "72ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.2.3:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "72ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.3:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "72ac9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.3:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    skip_fmt! {ztimeout!(async {
        loop {
            if z9110.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9120.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9130.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9210.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9220.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9230.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9310.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9320.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9330.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })};

    let s = STORAGE.lock();

    for zid in [
        "72ac9110", "72ac9120", "72ac9130", "72ac9210", "72ac9220", "72ac9230", "72ac9310",
        "72ac9320", "72ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "72ad9110")
        .multicast("224.7.2.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "72ad9120")
        .multicast("224.7.2.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "72ad9130")
        .multicast("224.7.2.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "72ad9210")
        .multicast("224.7.2.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "72ad9220")
        .multicast("224.7.2.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "72ad9230")
        .multicast("224.7.2.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "72ad9310")
        .multicast("224.7.2.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "72ad9320")
        .multicast("224.7.2.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "72ad9330")
        .multicast("224.7.2.4:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();

        let q9110 = z9110.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9120 = z9120.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9130 = z9130.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9210 = z9210.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9220 = z9220.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9230 = z9230.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9310 = z9310.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9320 = z9320.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9330 = z9330.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
    }

    let z9100 = ztimeout!(Node::new(Router, "72ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.2.4:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "72ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "72ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.4:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "72ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.2.4:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "72ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.4:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "72ad9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.4:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            if q9110.get().await.unwrap().iter().count() == 9
                && q9120.get().await.unwrap().iter().count() == 9
                && q9130.get().await.unwrap().iter().count() == 9
                && q9210.get().await.unwrap().iter().count() == 9
                && q9220.get().await.unwrap().iter().count() == 9
                && q9230.get().await.unwrap().iter().count() == 9
                && q9310.get().await.unwrap().iter().count() == 9
                && q9320.get().await.unwrap().iter().count() == 9
                && q9330.get().await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "72ad9110", "72ad9120", "72ad9130", "72ad9210", "72ad9220", "72ad9230", "72ad9310",
        "72ad9320", "72ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order2_toksub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "72ae9110")
        .multicast("224.7.2.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "72ae9120")
        .multicast("224.7.2.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "72ae9130")
        .multicast("224.7.2.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "72ae9210")
        .multicast("224.7.2.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "72ae9220")
        .multicast("224.7.2.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "72ae9230")
        .multicast("224.7.2.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "72ae9310")
        .multicast("224.7.2.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "72ae9320")
        .multicast("224.7.2.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "72ae9330")
        .multicast("224.7.2.5:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let z9100 = ztimeout!(Node::new(Router, "72ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.2.5:9100")
        .open());
    let _z9101 = ztimeout!(Node::new(Router, "72ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.5:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "72ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.2.5:9200")
        .open());
    let _z9201 = ztimeout!(Node::new(Router, "72ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.2.5:9200")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "72ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.5:9300")
        .open());
    let _z9301 = ztimeout!(Node::new(Router, "72ae9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.2.5:9300")
        .open());

    let t9110 = z9110.liveliness().declare_token("test/9110").await.unwrap();
    let t9120 = z9120.liveliness().declare_token("test/9120").await.unwrap();
    let t9130 = z9130.liveliness().declare_token("test/9130").await.unwrap();
    let t9210 = z9210.liveliness().declare_token("test/9210").await.unwrap();
    let t9220 = z9220.liveliness().declare_token("test/9220").await.unwrap();
    let t9230 = z9230.liveliness().declare_token("test/9230").await.unwrap();
    let t9310 = z9310.liveliness().declare_token("test/9310").await.unwrap();
    let t9320 = z9320.liveliness().declare_token("test/9320").await.unwrap();
    let t9330 = z9330.liveliness().declare_token("test/9330").await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    t9110.undeclare().await.unwrap();
    t9120.undeclare().await.unwrap();
    t9130.undeclare().await.unwrap();
    t9210.undeclare().await.unwrap();
    t9220.undeclare().await.unwrap();
    t9230.undeclare().await.unwrap();
    t9310.undeclare().await.unwrap();
    t9320.undeclare().await.unwrap();
    t9330.undeclare().await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Delete) == 9)
            {
                break;
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order3_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "73aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.3.1:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "73aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "73aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.1:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "73aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.3.1:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "73aa9110")
        .multicast("224.7.3.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "73aa9120")
        .multicast("224.7.3.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "73aa9130")
        .multicast("224.7.3.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "73aa9210")
        .multicast("224.7.3.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "73aa9220")
        .multicast("224.7.3.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "73aa9230")
        .multicast("224.7.3.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "73aa9310")
        .multicast("224.7.3.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "73aa9320")
        .multicast("224.7.3.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "73aa9330")
        .multicast("224.7.3.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "73aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.1:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "73aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.1:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            z9110.put("test", "9110").await.unwrap();
            z9120.put("test", "9120").await.unwrap();
            z9130.put("test", "9130").await.unwrap();
            z9210.put("test", "9210").await.unwrap();
            z9220.put("test", "9220").await.unwrap();
            z9230.put("test", "9230").await.unwrap();
            z9310.put("test", "9310").await.unwrap();
            z9320.put("test", "9320").await.unwrap();
            z9330.put("test", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "73aa9110", "73aa9120", "73aa9130", "73aa9210", "73aa9220", "73aa9230", "73aa9310",
        "73aa9320", "73aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order3_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "73ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.3.2:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "73ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "73ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.2:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "73ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.3.2:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "73ab9110")
        .multicast("224.7.3.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "73ab9120")
        .multicast("224.7.3.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "73ab9130")
        .multicast("224.7.3.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "73ab9210")
        .multicast("224.7.3.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "73ab9220")
        .multicast("224.7.3.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "73ab9230")
        .multicast("224.7.3.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "73ab9310")
        .multicast("224.7.3.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "73ab9320")
        .multicast("224.7.3.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "73ab9330")
        .multicast("224.7.3.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9110 = z9110.declare_publisher("test").await.unwrap();
    let p9120 = z9120.declare_publisher("test").await.unwrap();
    let p9130 = z9130.declare_publisher("test").await.unwrap();
    let p9210 = z9210.declare_publisher("test").await.unwrap();
    let p9220 = z9220.declare_publisher("test").await.unwrap();
    let p9230 = z9230.declare_publisher("test").await.unwrap();
    let p9310 = z9310.declare_publisher("test").await.unwrap();
    let p9320 = z9320.declare_publisher("test").await.unwrap();
    let p9330 = z9330.declare_publisher("test").await.unwrap();

    let z9300 = ztimeout!(Node::new(Router, "73ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.2:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "73ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.2:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            p9110.put("9110").await.unwrap();
            p9120.put("9120").await.unwrap();
            p9130.put("9130").await.unwrap();
            p9210.put("9210").await.unwrap();
            p9220.put("9220").await.unwrap();
            p9230.put("9230").await.unwrap();
            p9310.put("9310").await.unwrap();
            p9320.put("9320").await.unwrap();
            p9330.put("9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "73ab9110", "73ab9120", "73ab9130", "73ab9210", "73ab9220", "73ab9230", "73ab9310",
        "73ab9320", "73ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order3_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "73ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.3.3:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "73ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "73ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.3:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "73ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.3.3:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "73ac9110")
        .multicast("224.7.3.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "73ac9120")
        .multicast("224.7.3.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "73ac9130")
        .multicast("224.7.3.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "73ac9210")
        .multicast("224.7.3.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "73ac9220")
        .multicast("224.7.3.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "73ac9230")
        .multicast("224.7.3.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "73ac9310")
        .multicast("224.7.3.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "73ac9320")
        .multicast("224.7.3.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "73ac9330")
        .multicast("224.7.3.3:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "73ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.3:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "73ac9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.3:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    skip_fmt! {ztimeout!(async {
        loop {
            if z9110.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9120.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9130.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9210.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9220.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9230.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9310.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9320.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9330.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })};

    let s = STORAGE.lock();

    for zid in [
        "73ac9110", "73ac9120", "73ac9130", "73ac9210", "73ac9220", "73ac9230", "73ac9310",
        "73ac9320", "73ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order3_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "73ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.3.4:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "73ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "73ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.4:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "73ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.3.4:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "73ad9110")
        .multicast("224.7.3.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "73ad9120")
        .multicast("224.7.3.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "73ad9130")
        .multicast("224.7.3.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "73ad9210")
        .multicast("224.7.3.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "73ad9220")
        .multicast("224.7.3.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "73ad9230")
        .multicast("224.7.3.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "73ad9310")
        .multicast("224.7.3.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "73ad9320")
        .multicast("224.7.3.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "73ad9330")
        .multicast("224.7.3.4:9300")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();

        let q9110 = z9110.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9120 = z9120.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9130 = z9130.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9210 = z9210.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9220 = z9220.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9230 = z9230.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9310 = z9310.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9320 = z9320.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9330 = z9330.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "73ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.4:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "73ad9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.4:9300")
        .open());

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            if q9110.get().await.unwrap().iter().count() == 9
                && q9120.get().await.unwrap().iter().count() == 9
                && q9130.get().await.unwrap().iter().count() == 9
                && q9210.get().await.unwrap().iter().count() == 9
                && q9220.get().await.unwrap().iter().count() == 9
                && q9230.get().await.unwrap().iter().count() == 9
                && q9310.get().await.unwrap().iter().count() == 9
                && q9320.get().await.unwrap().iter().count() == 9
                && q9330.get().await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "73ad9110", "73ad9120", "73ad9130", "73ad9210", "73ad9220", "73ad9230", "73ad9310",
        "73ad9320", "73ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order3_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "73ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.3.5:9100")
        .open());
    let _z9101 = ztimeout!(Node::new(Router, "73ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.5:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "73ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.3.5:9200")
        .open());
    let _z9201 = ztimeout!(Node::new(Router, "73ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.3.5:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "73ae9110")
        .multicast("224.7.3.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "73ae9120")
        .multicast("224.7.3.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "73ae9130")
        .multicast("224.7.3.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "73ae9210")
        .multicast("224.7.3.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "73ae9220")
        .multicast("224.7.3.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "73ae9230")
        .multicast("224.7.3.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "73ae9310")
        .multicast("224.7.3.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "73ae9320")
        .multicast("224.7.3.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "73ae9330")
        .multicast("224.7.3.5:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9310 = z9310.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "73ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.5:9300")
        .open());
    let _z9301 = ztimeout!(Node::new(Router, "73ae9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.3.5:9300")
        .open());

    let t9110 = z9110.liveliness().declare_token("test/9110").await.unwrap();
    let t9120 = z9120.liveliness().declare_token("test/9120").await.unwrap();
    let t9130 = z9130.liveliness().declare_token("test/9130").await.unwrap();
    let t9210 = z9210.liveliness().declare_token("test/9210").await.unwrap();
    let t9220 = z9220.liveliness().declare_token("test/9220").await.unwrap();
    let t9230 = z9230.liveliness().declare_token("test/9230").await.unwrap();
    let t9310 = z9310.liveliness().declare_token("test/9310").await.unwrap();
    let t9320 = z9320.liveliness().declare_token("test/9320").await.unwrap();
    let t9330 = z9330.liveliness().declare_token("test/9330").await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    t9110.undeclare().await.unwrap();
    t9120.undeclare().await.unwrap();
    t9130.undeclare().await.unwrap();
    t9210.undeclare().await.unwrap();
    t9220.undeclare().await.unwrap();
    t9230.undeclare().await.unwrap();
    t9310.undeclare().await.unwrap();
    t9320.undeclare().await.unwrap();
    t9330.undeclare().await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Delete) == 9)
            {
                break;
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order4_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "74aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.4.1:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "74aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "74aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.1:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "74aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.4.1:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "74aa9110")
        .multicast("224.7.4.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "74aa9120")
        .multicast("224.7.4.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "74aa9130")
        .multicast("224.7.4.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "74aa9210")
        .multicast("224.7.4.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "74aa9220")
        .multicast("224.7.4.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "74aa9230")
        .multicast("224.7.4.1:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "74aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.1:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "74aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.1:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "74aa9310")
        .multicast("224.7.4.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "74aa9320")
        .multicast("224.7.4.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "74aa9330")
        .multicast("224.7.4.1:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            z9110.put("test", "9110").await.unwrap();
            z9120.put("test", "9120").await.unwrap();
            z9130.put("test", "9130").await.unwrap();
            z9210.put("test", "9210").await.unwrap();
            z9220.put("test", "9220").await.unwrap();
            z9230.put("test", "9230").await.unwrap();
            z9310.put("test", "9310").await.unwrap();
            z9320.put("test", "9320").await.unwrap();
            z9330.put("test", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "74aa9110", "74aa9120", "74aa9130", "74aa9210", "74aa9220", "74aa9230", "74aa9310",
        "74aa9320", "74aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order4_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "74ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.4.2:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "74ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "74ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.2:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "74ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.4.2:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "74ab9110")
        .multicast("224.7.4.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "74ab9120")
        .multicast("224.7.4.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "74ab9130")
        .multicast("224.7.4.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "74ab9210")
        .multicast("224.7.4.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "74ab9220")
        .multicast("224.7.4.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "74ab9230")
        .multicast("224.7.4.2:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9110 = z9110.declare_publisher("test").await.unwrap();
    let p9120 = z9120.declare_publisher("test").await.unwrap();
    let p9130 = z9130.declare_publisher("test").await.unwrap();
    let p9210 = z9210.declare_publisher("test").await.unwrap();
    let p9220 = z9220.declare_publisher("test").await.unwrap();
    let p9230 = z9230.declare_publisher("test").await.unwrap();

    let z9300 = ztimeout!(Node::new(Router, "74ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.2:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "74ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.2:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "74ab9310")
        .multicast("224.7.4.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "74ab9320")
        .multicast("224.7.4.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "74ab9330")
        .multicast("224.7.4.2:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9310 = z9310.declare_publisher("test").await.unwrap();
    let p9320 = z9320.declare_publisher("test").await.unwrap();
    let p9330 = z9330.declare_publisher("test").await.unwrap();

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            p9110.put("9110").await.unwrap();
            p9120.put("9120").await.unwrap();
            p9130.put("9130").await.unwrap();
            p9210.put("9210").await.unwrap();
            p9220.put("9220").await.unwrap();
            p9230.put("9230").await.unwrap();
            p9310.put("9310").await.unwrap();
            p9320.put("9320").await.unwrap();
            p9330.put("9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_payload(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    assert!(
        [&s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330]
            .iter()
            .all(|sub| sub.unique_timestamps())
    );

    let s = STORAGE.lock();

    for zid in [
        "74ab9110", "74ab9120", "74ab9130", "74ab9210", "74ab9220", "74ab9230", "74ab9310",
        "74ab9320", "74ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order4_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "74ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.4.3:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "74ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "74ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.3:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "74ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.4.3:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "74ac9110")
        .multicast("224.7.4.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "74ac9120")
        .multicast("224.7.4.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "74ac9130")
        .multicast("224.7.4.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "74ac9210")
        .multicast("224.7.4.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "74ac9220")
        .multicast("224.7.4.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "74ac9230")
        .multicast("224.7.4.3:9200")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "74ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.3:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "74ac9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.3:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "74ac9310")
        .multicast("224.7.4.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "74ac9320")
        .multicast("224.7.4.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "74ac9330")
        .multicast("224.7.4.3:9300")
        .open());

    skip_fmt! {
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    skip_fmt! {ztimeout!(async {
        loop {
            if z9110.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9120.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9130.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9210.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9220.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9230.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9310.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9320.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
                && z9330.get("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })};

    let s = STORAGE.lock();

    for zid in [
        "74ac9110", "74ac9120", "74ac9130", "74ac9210", "74ac9220", "74ac9230", "74ac9310",
        "74ac9320", "74ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order4_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "74ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.4.4:9100")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "74ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "74ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.4:9200")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "74ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.4.4:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "74ad9110")
        .multicast("224.7.4.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "74ad9120")
        .multicast("224.7.4.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "74ad9130")
        .multicast("224.7.4.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "74ad9210")
        .multicast("224.7.4.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "74ad9220")
        .multicast("224.7.4.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "74ad9230")
        .multicast("224.7.4.4:9200")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();

        let q9110 = z9110.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9120 = z9120.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9130 = z9130.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9210 = z9210.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9220 = z9220.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9230 = z9230.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "74ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.4:9300")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "74ad9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.4:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "74ad9310")
        .multicast("224.7.4.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "74ad9320")
        .multicast("224.7.4.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "74ad9330")
        .multicast("224.7.4.4:9300")
        .open());

    skip_fmt! {
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();

        let q9310 = z9310.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9320 = z9320.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
        let q9330 = z9330.declare_querier("test").target(QueryTarget::All).consolidation(ConsolidationMode::None).await.unwrap();
    }

    for z in [&z9100, &z9101, &z9200, &z9201, &z9300, &z9301] {
        wait_for_n_peers(z, 3).await;
    }

    ztimeout!(async {
        loop {
            if q9110.get().await.unwrap().iter().count() == 9
                && q9120.get().await.unwrap().iter().count() == 9
                && q9130.get().await.unwrap().iter().count() == 9
                && q9210.get().await.unwrap().iter().count() == 9
                && q9220.get().await.unwrap().iter().count() == 9
                && q9230.get().await.unwrap().iter().count() == 9
                && q9310.get().await.unwrap().iter().count() == 9
                && q9320.get().await.unwrap().iter().count() == 9
                && q9330.get().await.unwrap().iter().count() == 9
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "74ad9110", "74ad9120", "74ad9130", "74ad9210", "74ad9220", "74ad9230", "74ad9310",
        "74ad9320", "74ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            4
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario7_order4_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "74ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.7.4.5:9100")
        .open());
    let _z9101 = ztimeout!(Node::new(Router, "74ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.5:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "74ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.7.4.5:9200")
        .open());
    let _z9201 = ztimeout!(Node::new(Router, "74ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .multicast("224.7.4.5:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "74ae9110")
        .multicast("224.7.4.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "74ae9120")
        .multicast("224.7.4.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "74ae9130")
        .multicast("224.7.4.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "74ae9210")
        .multicast("224.7.4.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "74ae9220")
        .multicast("224.7.4.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "74ae9230")
        .multicast("224.7.4.5:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "74ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.5:9300")
        .open());
    let _z9301 = ztimeout!(Node::new(Router, "74ae9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .multicast("224.7.4.5:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "74ae9310")
        .multicast("224.7.4.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "74ae9320")
        .multicast("224.7.4.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "74ae9330")
        .multicast("224.7.4.5:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let t9110 = z9110.liveliness().declare_token("test/9110").await.unwrap();
    let t9120 = z9120.liveliness().declare_token("test/9120").await.unwrap();
    let t9130 = z9130.liveliness().declare_token("test/9130").await.unwrap();
    let t9210 = z9210.liveliness().declare_token("test/9210").await.unwrap();
    let t9220 = z9220.liveliness().declare_token("test/9220").await.unwrap();
    let t9230 = z9230.liveliness().declare_token("test/9230").await.unwrap();
    let t9310 = z9310.liveliness().declare_token("test/9310").await.unwrap();
    let t9320 = z9320.liveliness().declare_token("test/9320").await.unwrap();
    let t9330 = z9330.liveliness().declare_token("test/9330").await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Put) == 9)
            {
                break;
            }
        }
    });

    t9110.undeclare().await.unwrap();
    t9120.undeclare().await.unwrap();
    t9130.undeclare().await.unwrap();
    t9210.undeclare().await.unwrap();
    t9220.undeclare().await.unwrap();
    t9230.undeclare().await.unwrap();
    t9310.undeclare().await.unwrap();
    t9320.undeclare().await.unwrap();
    t9330.undeclare().await.unwrap();

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if [
                &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
            ]
            .iter()
            .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Delete) == 9)
            {
                break;
            }
        }
    });
}

#[tracing::instrument(level = "debug", ret)]
async fn wait_for_n_peers(z: &zenoh::Session, n: usize) {
    let listener = z
        .info()
        .transport_events_listener()
        .history(true)
        .await
        .unwrap();
    let mut stream = listener.stream();

    let mut count = 0;
    while let Some(event) = stream.next().await {
        assert_eq!(event.kind(), SampleKind::Put);

        if event.transport().whatami().is_peer() {
            count += 1;
        }

        assert!(count <= n);

        if count == n {
            break;
        }
    }
}
