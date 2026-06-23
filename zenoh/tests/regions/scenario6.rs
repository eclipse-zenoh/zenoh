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

// Scenario 6
//          R
// P P P  P P P  P P P

use std::time::Duration;

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
async fn test_regions_scenario6_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "61aa9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]},
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "61aa9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "61aa9120")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "61aa9130")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "61aa9210")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "61aa9220")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "61aa9230")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "61aa9310")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "61aa9320")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "61aa9330")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.1:9300")
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
        "61aa9110", "61aa9120", "61aa9130", "61aa9210", "61aa9220", "61aa9230", "61aa9310",
        "61aa9320", "61aa9330",
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
async fn test_regions_scenario6_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "61ab9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]},
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "61ab9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "61ab9120")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "61ab9130")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "61ab9210")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "61ab9220")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "61ab9230")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "61ab9310")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "61ab9320")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "61ab9330")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.2:9300")
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
        "61ab9110", "61ab9120", "61ab9130", "61ab9210", "61ab9220", "61ab9230", "61ab9310",
        "61ab9320", "61ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario6_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "61ac9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]},
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "61ac9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "61ac9120")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "61ac9130")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "61ac9210")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "61ac9220")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "61ac9230")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "61ac9310")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "61ac9320")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "61ac9330")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.3:9300")
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
        "61ac9110", "61ac9120", "61ac9130", "61ac9210", "61ac9220", "61ac9230", "61ac9310",
        "61ac9320", "61ac9330",
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
async fn test_regions_scenario6_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "61ad9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]},
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "61ad9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "61ad9120")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "61ad9130")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "61ad9210")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "61ad9220")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "61ad9230")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "61ad9310")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "61ad9320")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "61ad9330")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.4:9300")
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
        "61ad9110", "61ad9120", "61ad9130", "61ad9210", "61ad9220", "61ad9230", "61ad9310",
        "61ad9320", "61ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario6_order1_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "61ae9100")
        .listen("tcp/0.0.0.0:0")
        .gateway(
            "{south:[
            {filters:[{region_names:[\"region1\"]}]},
            {filters:[{region_names:[\"region2\"]}]},
            {filters:[{region_names:[\"region3\"]}]}
        ]}"
        )
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "61ae9110")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "61ae9120")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "61ae9130")
        .region("region1")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "61ae9210")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "61ae9220")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "61ae9230")
        .region("region2")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "61ae9310")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "61ae9320")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "61ae9330")
        .region("region3")
        .connect(&[loc!(z9100)])
        .multicast("224.6.1.5:9300")
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
