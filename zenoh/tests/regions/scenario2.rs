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

// Scenario 2
//   P      P      P
// P P P  P P P  P P P

use std::time::Duration;

use predicates::Predicate;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    sample::SampleKind,
    Wait,
};
use zenoh_config::WhatAmI::Peer;
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
async fn test_regions_scenario2_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "21aa9100")
        .multicast("224.2.1.1:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "21aa9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.1.1:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21aa9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.1.1:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "21aa9110")
        .multicast("224.2.1.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "21aa9120")
        .multicast("224.2.1.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "21aa9130")
        .multicast("224.2.1.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "21aa9210")
        .multicast("224.2.1.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "21aa9220")
        .multicast("224.2.1.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "21aa9230")
        .multicast("224.2.1.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "21aa9310")
        .multicast("224.2.1.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "21aa9320")
        .multicast("224.2.1.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "21aa9330")
        .multicast("224.2.1.1:9300")
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
        "21aa9110", "21aa9120", "21aa9130", "21aa9210", "21aa9220", "21aa9230", "21aa9310",
        "21aa9320", "21aa9330",
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
async fn test_regions_scenario2_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "21ab9100")
        .multicast("224.2.1.2:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "21ab9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.1.2:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21ab9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.1.2:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "21ab9110")
        .multicast("224.2.1.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "21ab9120")
        .multicast("224.2.1.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "21ab9130")
        .multicast("224.2.1.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "21ab9210")
        .multicast("224.2.1.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "21ab9220")
        .multicast("224.2.1.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "21ab9230")
        .multicast("224.2.1.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "21ab9310")
        .multicast("224.2.1.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "21ab9320")
        .multicast("224.2.1.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "21ab9330")
        .multicast("224.2.1.2:9300")
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
        "21ab9110", "21ab9120", "21ab9130", "21ab9210", "21ab9220", "21ab9230", "21ab9310",
        "21ab9320", "21ab9330",
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
async fn test_regions_scenario2_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "21ac9100")
        .multicast("224.2.1.3:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "21ac9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.1.3:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21ac9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.1.3:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "21ac9110")
        .multicast("224.2.1.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "21ac9120")
        .multicast("224.2.1.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "21ac9130")
        .multicast("224.2.1.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "21ac9210")
        .multicast("224.2.1.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "21ac9220")
        .multicast("224.2.1.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "21ac9230")
        .multicast("224.2.1.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "21ac9310")
        .multicast("224.2.1.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "21ac9320")
        .multicast("224.2.1.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "21ac9330")
        .multicast("224.2.1.3:9300")
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
        "21ac9110", "21ac9120", "21ac9130", "21ac9210", "21ac9220", "21ac9230", "21ac9310",
        "21ac9320", "21ac9330",
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
async fn test_regions_scenario2_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "21ad9100")
        .multicast("224.2.1.4:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "21ad9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.1.4:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21ad9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.1.4:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "21ad9110")
        .multicast("224.2.1.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "21ad9120")
        .multicast("224.2.1.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "21ad9130")
        .multicast("224.2.1.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "21ad9210")
        .multicast("224.2.1.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "21ad9220")
        .multicast("224.2.1.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "21ad9230")
        .multicast("224.2.1.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "21ad9310")
        .multicast("224.2.1.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "21ad9320")
        .multicast("224.2.1.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "21ad9330")
        .multicast("224.2.1.4:9300")
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
        "21ad9110", "21ad9120", "21ad9130", "21ad9210", "21ad9220", "21ad9230", "21ad9310",
        "21ad9320", "21ad9330",
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
async fn test_regions_scenario2_order1_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "21ae9100")
        .multicast("224.2.1.5:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9200 = ztimeout!(Node::new(Peer, "21ae9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.1.5:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let _z9300 = ztimeout!(Node::new(Peer, "21ae9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.1.5:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "21ae9110")
        .multicast("224.2.1.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "21ae9120")
        .multicast("224.2.1.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "21ae9130")
        .multicast("224.2.1.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "21ae9210")
        .multicast("224.2.1.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "21ae9220")
        .multicast("224.2.1.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "21ae9230")
        .multicast("224.2.1.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "21ae9310")
        .multicast("224.2.1.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "21ae9320")
        .multicast("224.2.1.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "21ae9330")
        .multicast("224.2.1.5:9300")
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
async fn test_regions_scenario2_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "22aa9110")
        .multicast("224.2.2.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "22aa9120")
        .multicast("224.2.2.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "22aa9130")
        .multicast("224.2.2.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "22aa9210")
        .multicast("224.2.2.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "22aa9220")
        .multicast("224.2.2.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "22aa9230")
        .multicast("224.2.2.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "22aa9310")
        .multicast("224.2.2.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "22aa9320")
        .multicast("224.2.2.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "22aa9330")
        .multicast("224.2.2.1:9300")
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

    let z9100 = ztimeout!(Node::new(Peer, "22aa9100")
        .multicast("224.2.2.1:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "22aa9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.2.1:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22aa9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.2.1:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "22aa9110", "22aa9120", "22aa9130", "22aa9210", "22aa9220", "22aa9230", "22aa9310",
        "22aa9320", "22aa9330",
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
async fn test_regions_scenario2_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "22ab9110")
        .multicast("224.2.2.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "22ab9120")
        .multicast("224.2.2.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "22ab9130")
        .multicast("224.2.2.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "22ab9210")
        .multicast("224.2.2.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "22ab9220")
        .multicast("224.2.2.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "22ab9230")
        .multicast("224.2.2.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "22ab9310")
        .multicast("224.2.2.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "22ab9320")
        .multicast("224.2.2.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "22ab9330")
        .multicast("224.2.2.2:9300")
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

    let z9100 = ztimeout!(Node::new(Peer, "22ab9100")
        .multicast("224.2.2.2:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "22ab9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.2.2:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22ab9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.2.2:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "22ab9110", "22ab9120", "22ab9130", "22ab9210", "22ab9220", "22ab9230", "22ab9310",
        "22ab9320", "22ab9330",
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
async fn test_regions_scenario2_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "22ac9110")
        .multicast("224.2.2.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "22ac9120")
        .multicast("224.2.2.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "22ac9130")
        .multicast("224.2.2.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "22ac9210")
        .multicast("224.2.2.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "22ac9220")
        .multicast("224.2.2.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "22ac9230")
        .multicast("224.2.2.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "22ac9310")
        .multicast("224.2.2.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "22ac9320")
        .multicast("224.2.2.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "22ac9330")
        .multicast("224.2.2.3:9300")
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

    let z9100 = ztimeout!(Node::new(Peer, "22ac9100")
        .multicast("224.2.2.3:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "22ac9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.2.3:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22ac9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.2.3:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "22ac9110", "22ac9120", "22ac9130", "22ac9210", "22ac9220", "22ac9230", "22ac9310",
        "22ac9320", "22ac9330",
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
async fn test_regions_scenario2_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "22ad9110")
        .multicast("224.2.2.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "22ad9120")
        .multicast("224.2.2.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "22ad9130")
        .multicast("224.2.2.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "22ad9210")
        .multicast("224.2.2.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "22ad9220")
        .multicast("224.2.2.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "22ad9230")
        .multicast("224.2.2.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "22ad9310")
        .multicast("224.2.2.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "22ad9320")
        .multicast("224.2.2.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "22ad9330")
        .multicast("224.2.2.4:9300")
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

    let z9100 = ztimeout!(Node::new(Peer, "22ad9100")
        .multicast("224.2.2.4:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "22ad9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.2.4:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22ad9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.2.4:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "22ad9110", "22ad9120", "22ad9130", "22ad9210", "22ad9220", "22ad9230", "22ad9310",
        "22ad9320", "22ad9330",
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
async fn test_regions_scenario2_order2_toksub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "22ae9110")
        .multicast("224.2.2.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "22ae9120")
        .multicast("224.2.2.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "22ae9130")
        .multicast("224.2.2.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "22ae9210")
        .multicast("224.2.2.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "22ae9220")
        .multicast("224.2.2.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "22ae9230")
        .multicast("224.2.2.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "22ae9310")
        .multicast("224.2.2.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "22ae9320")
        .multicast("224.2.2.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "22ae9330")
        .multicast("224.2.2.5:9300")
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

    let z9100 = ztimeout!(Node::new(Peer, "22ae9100")
        .multicast("224.2.2.5:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9200 = ztimeout!(Node::new(Peer, "22ae9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.2.5:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let _z9300 = ztimeout!(Node::new(Peer, "22ae9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.2.5:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
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
async fn test_regions_scenario2_order3_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "23aa9100")
        .multicast("224.2.3.1:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "23aa9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.3.1:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "23aa9110")
        .multicast("224.2.3.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "23aa9120")
        .multicast("224.2.3.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "23aa9130")
        .multicast("224.2.3.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "23aa9210")
        .multicast("224.2.3.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "23aa9220")
        .multicast("224.2.3.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "23aa9230")
        .multicast("224.2.3.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "23aa9310")
        .multicast("224.2.3.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "23aa9320")
        .multicast("224.2.3.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "23aa9330")
        .multicast("224.2.3.1:9300")
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

    let _z9300 = ztimeout!(Node::new(Peer, "23aa9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.3.1:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "23aa9110", "23aa9120", "23aa9130", "23aa9210", "23aa9220", "23aa9230", "23aa9310",
        "23aa9320", "23aa9330",
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
async fn test_regions_scenario2_order3_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "23ab9100")
        .multicast("224.2.3.2:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "23ab9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.3.2:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "23ab9110")
        .multicast("224.2.3.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "23ab9120")
        .multicast("224.2.3.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "23ab9130")
        .multicast("224.2.3.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "23ab9210")
        .multicast("224.2.3.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "23ab9220")
        .multicast("224.2.3.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "23ab9230")
        .multicast("224.2.3.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "23ab9310")
        .multicast("224.2.3.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "23ab9320")
        .multicast("224.2.3.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "23ab9330")
        .multicast("224.2.3.2:9300")
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

    let _z9300 = ztimeout!(Node::new(Peer, "23ab9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.3.2:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "23ab9110", "23ab9120", "23ab9130", "23ab9210", "23ab9220", "23ab9230", "23ab9310",
        "23ab9320", "23ab9330",
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
async fn test_regions_scenario2_order3_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "23ac9100")
        .multicast("224.2.3.3:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "23ac9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.3.3:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "23ac9110")
        .multicast("224.2.3.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "23ac9120")
        .multicast("224.2.3.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "23ac9130")
        .multicast("224.2.3.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "23ac9210")
        .multicast("224.2.3.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "23ac9220")
        .multicast("224.2.3.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "23ac9230")
        .multicast("224.2.3.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "23ac9310")
        .multicast("224.2.3.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "23ac9320")
        .multicast("224.2.3.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "23ac9330")
        .multicast("224.2.3.3:9300")
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

    let _z9300 = ztimeout!(Node::new(Peer, "23ac9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.3.3:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "23ac9110", "23ac9120", "23ac9130", "23ac9210", "23ac9220", "23ac9230", "23ac9310",
        "23ac9320", "23ac9330",
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
async fn test_regions_scenario2_order3_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "23ad9100")
        .multicast("224.2.3.4:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "23ad9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.3.4:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "23ad9110")
        .multicast("224.2.3.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "23ad9120")
        .multicast("224.2.3.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "23ad9130")
        .multicast("224.2.3.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "23ad9210")
        .multicast("224.2.3.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "23ad9220")
        .multicast("224.2.3.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "23ad9230")
        .multicast("224.2.3.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "23ad9310")
        .multicast("224.2.3.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "23ad9320")
        .multicast("224.2.3.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "23ad9330")
        .multicast("224.2.3.4:9300")
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

    let _z9300 = ztimeout!(Node::new(Peer, "23ad9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.3.4:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

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
        "23ad9110", "23ad9120", "23ad9130", "23ad9210", "23ad9220", "23ad9230", "23ad9310",
        "23ad9320", "23ad9330",
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
async fn test_regions_scenario2_order3_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "23ae9100")
        .multicast("224.2.3.5:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9200 = ztimeout!(Node::new(Peer, "23ae9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.3.5:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "23ae9110")
        .multicast("224.2.3.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "23ae9120")
        .multicast("224.2.3.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "23ae9130")
        .multicast("224.2.3.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "23ae9210")
        .multicast("224.2.3.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "23ae9220")
        .multicast("224.2.3.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "23ae9230")
        .multicast("224.2.3.5:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "23ae9310")
        .multicast("224.2.3.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "23ae9320")
        .multicast("224.2.3.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "23ae9330")
        .multicast("224.2.3.5:9300")
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

    let _z9300 = ztimeout!(Node::new(Peer, "23ae9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.3.5:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
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
async fn test_regions_scenario2_order4_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "24aa9100")
        .multicast("224.2.4.1:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "24aa9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.4.1:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "24aa9110")
        .multicast("224.2.4.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "24aa9120")
        .multicast("224.2.4.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "24aa9130")
        .multicast("224.2.4.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "24aa9210")
        .multicast("224.2.4.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "24aa9220")
        .multicast("224.2.4.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "24aa9230")
        .multicast("224.2.4.1:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Peer, "24aa9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.4.1:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "24aa9310")
        .multicast("224.2.4.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "24aa9320")
        .multicast("224.2.4.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "24aa9330")
        .multicast("224.2.4.1:9300")
        .open());

    skip_fmt! {
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
        "24aa9110", "24aa9120", "24aa9130", "24aa9210", "24aa9220", "24aa9230", "24aa9310",
        "24aa9320", "24aa9330",
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
async fn test_regions_scenario2_order4_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "24ab9100")
        .multicast("224.2.4.2:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "24ab9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.4.2:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "24ab9110")
        .multicast("224.2.4.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "24ab9120")
        .multicast("224.2.4.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "24ab9130")
        .multicast("224.2.4.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "24ab9210")
        .multicast("224.2.4.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "24ab9220")
        .multicast("224.2.4.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "24ab9230")
        .multicast("224.2.4.2:9200")
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

    let _z9300 = ztimeout!(Node::new(Peer, "24ab9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.4.2:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "24ab9310")
        .multicast("224.2.4.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "24ab9320")
        .multicast("224.2.4.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "24ab9330")
        .multicast("224.2.4.2:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

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
        "24ab9110", "24ab9120", "24ab9130", "24ab9210", "24ab9220", "24ab9230", "24ab9310",
        "24ab9320", "24ab9330",
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
async fn test_regions_scenario2_order4_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "24ac9100")
        .multicast("224.2.4.3:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "24ac9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.4.3:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "24ac9110")
        .multicast("224.2.4.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "24ac9120")
        .multicast("224.2.4.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "24ac9130")
        .multicast("224.2.4.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "24ac9210")
        .multicast("224.2.4.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "24ac9220")
        .multicast("224.2.4.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "24ac9230")
        .multicast("224.2.4.3:9200")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Peer, "24ac9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.4.3:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "24ac9310")
        .multicast("224.2.4.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "24ac9320")
        .multicast("224.2.4.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "24ac9330")
        .multicast("224.2.4.3:9300")
        .open());

    skip_fmt! {
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
        "24ac9110", "24ac9120", "24ac9130", "24ac9210", "24ac9220", "24ac9230", "24ac9310",
        "24ac9320", "24ac9330",
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
async fn test_regions_scenario2_order4_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "24ad9100")
        .multicast("224.2.4.4:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Peer, "24ad9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.4.4:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "24ad9110")
        .multicast("224.2.4.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "24ad9120")
        .multicast("224.2.4.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "24ad9130")
        .multicast("224.2.4.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "24ad9210")
        .multicast("224.2.4.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "24ad9220")
        .multicast("224.2.4.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "24ad9230")
        .multicast("224.2.4.4:9200")
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

    let _z9300 = ztimeout!(Node::new(Peer, "24ad9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.4.4:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "24ad9310")
        .multicast("224.2.4.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "24ad9320")
        .multicast("224.2.4.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "24ad9330")
        .multicast("224.2.4.4:9300")
        .open());

    skip_fmt! {
        let _q9310 = z9310.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9310")).unwrap()).await.unwrap();
        let _q9320 = z9320.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9320")).unwrap()).await.unwrap();
        let _q9330 = z9330.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9330")).unwrap()).await.unwrap();

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
        "24ad9110", "24ad9120", "24ad9130", "24ad9210", "24ad9220", "24ad9230", "24ad9310",
        "24ad9320", "24ad9330",
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
async fn test_regions_scenario2_order4_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Peer, "24ae9100")
        .multicast("224.2.4.5:9100")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9200 = ztimeout!(Node::new(Peer, "24ae9200")
        .connect(&[loc!(z9100)])
        .multicast("224.2.4.5:9200")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "24ae9110")
        .multicast("224.2.4.5:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "24ae9120")
        .multicast("224.2.4.5:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "24ae9130")
        .multicast("224.2.4.5:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "24ae9210")
        .multicast("224.2.4.5:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "24ae9220")
        .multicast("224.2.4.5:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "24ae9230")
        .multicast("224.2.4.5:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Peer, "24ae9300")
        .connect(&[loc!(z9100), loc!(z9200)])
        .multicast("224.2.4.5:9300")
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "24ae9310")
        .multicast("224.2.4.5:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "24ae9320")
        .multicast("224.2.4.5:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "24ae9330")
        .multicast("224.2.4.5:9300")
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
