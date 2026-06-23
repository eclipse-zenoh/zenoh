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

// Scenario 5
//   R      R      R
// R R R  R R R  R R R

use std::time::Duration;

use predicates::Predicate;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    sample::SampleKind,
    Wait,
};
use zenoh_config::WhatAmI::Router;
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
async fn test_regions_scenario5_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "51aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "51aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "51aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "51aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "51aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "51aa9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "51aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "51aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "51aa9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "51aa9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "51aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "51aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "51aa9110", "51aa9120", "51aa9130", "51aa9210", "51aa9220", "51aa9230", "51aa9310",
        "51aa9320", "51aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3, // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "51ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "51ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "51ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "51ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "51ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "51ab9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "51ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "51ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "51ab9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "51ab9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "51ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "51ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "51ab9110", "51ab9120", "51ab9130", "51ab9210", "51ab9220", "51ab9230", "51ab9310",
        "51ab9320", "51ab9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3,
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "51ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "51ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "51ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "51ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "51ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "51ac9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "51ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "51ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "51ac9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "51ac9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "51ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "51ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "51ac9110", "51ac9120", "51ac9130", "51ac9210", "51ac9220", "51ac9230", "51ac9310",
        "51ac9320", "51ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3, // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "51ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "51ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "51ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "51ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "51ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "51ad9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "51ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "51ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "51ad9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "51ad9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "51ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "51ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "51ad9110", "51ad9120", "51ad9130", "51ad9210", "51ad9220", "51ad9230", "51ad9310",
        "51ad9320", "51ad9330",
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
async fn test_regions_scenario5_order1_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "51ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "51ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "51ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "51ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "51ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "51ae9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "51ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "51ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "51ae9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "51ae9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "51ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "51ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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

    let subs = [
        &s9110, &s9120, &s9130, &s9210, &s9220, &s9230, &s9310, &s9320, &s9330,
    ];

    ztimeout!(async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if subs
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

            if subs
                .iter()
                .all(|sub| sub.count_unique_by_keyexpr(SampleKind::Delete) == 9)
            {
                break;
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "52aa9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "52aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "52aa9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "52aa9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "52aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "52aa9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "52aa9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "52aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "52aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "52aa9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "52aa9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "52aa9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "52aa9110", "52aa9120", "52aa9130", "52aa9210", "52aa9220", "52aa9230", "52aa9310",
        "52aa9320", "52aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3 // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "52ab9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "52ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "52ab9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "52ab9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "52ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "52ab9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "52ab9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "52ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "52ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "52ab9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "52ab9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "52ab9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "52ab9110", "52ab9120", "52ab9130", "52ab9210", "52ab9220", "52ab9230", "52ab9310",
        "52ab9320", "52ab9330",
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
async fn test_regions_scenario5_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "52ac9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "52ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "52ac9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "52ac9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "52ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "52ac9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "52ac9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "52ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "52ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "52ac9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "52ac9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "52ac9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "52ac9110", "52ac9120", "52ac9130", "52ac9210", "52ac9220", "52ac9230", "52ac9310",
        "52ac9320", "52ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3 // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "52ad9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "52ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "52ad9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "52ad9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "52ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "52ad9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "52ad9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "52ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "52ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "52ad9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "52ad9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "52ad9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "52ad9110", "52ad9120", "52ad9130", "52ad9210", "52ad9220", "52ad9230", "52ad9310",
        "52ad9320", "52ad9330",
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
async fn test_regions_scenario5_order2_toksub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "52ae9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "52ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "52ae9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "52ae9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "52ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "52ae9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "52ae9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "52ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "52ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "52ae9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "52ae9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "52ae9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
async fn test_regions_scenario5_order3_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "53aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "53aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "53aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "53aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "53aa9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "53aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "53aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "53aa9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "53aa9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "53aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "53aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let _z9300 = ztimeout!(Node::new(Router, "53aa9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "53aa9110", "53aa9120", "53aa9130", "53aa9210", "53aa9220", "53aa9230", "53aa9310",
        "53aa9320", "53aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3 // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order3_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "53ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "53ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "53ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "53ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "53ab9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "53ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "53ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "53ab9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "53ab9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "53ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "53ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let _z9300 = ztimeout!(Node::new(Router, "53ab9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "53ab9110", "53ab9120", "53ab9130", "53ab9210", "53ab9220", "53ab9230", "53ab9310",
        "53ab9320", "53ab9330",
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
async fn test_regions_scenario5_order3_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "53ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "53ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "53ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "53ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "53ac9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "53ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "53ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "53ac9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "53ac9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "53ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "53ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let _z9300 = ztimeout!(Node::new(Router, "53ac9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "53ac9110", "53ac9120", "53ac9130", "53ac9210", "53ac9220", "53ac9230", "53ac9310",
        "53ac9320", "53ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3, // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order3_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "53ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "53ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "53ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "53ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "53ad9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "53ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "53ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "53ad9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "53ad9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "53ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "53ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let _z9300 = ztimeout!(Node::new(Router, "53ad9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
        "53ad9110", "53ad9120", "53ad9130", "53ad9210", "53ad9220", "53ad9230", "53ad9310",
        "53ad9320", "53ad9330",
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
async fn test_regions_scenario5_order3_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "53ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "53ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "53ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "53ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "53ae9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "53ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "53ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "53ae9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "53ae9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "53ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "53ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310), loc!(z9320)])
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

    let _z9300 = ztimeout!(Node::new(Router, "53ae9300")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9100),
                loc!(z9200),
                loc!(z9310),
                loc!(z9320),
                loc!(z9330)
            ]
        )
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
async fn test_regions_scenario5_order4_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "54aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "54aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "54aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "54aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "54aa9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "54aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "54aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "54aa9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "54aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "54aa9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "54aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "54aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "54aa9110", "54aa9120", "54aa9130", "54aa9210", "54aa9220", "54aa9230", "54aa9310",
        "54aa9320", "54aa9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_subscriber(zid, "test").eval(e))
                .count(),
            3, // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order4_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "54ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "54ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "54ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "54ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "54ab9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "54ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "54ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "54ab9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
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

    let z9300 = ztimeout!(Node::new(Router, "54ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "54ab9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "54ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "54ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "54ab9110", "54ab9120", "54ab9130", "54ab9210", "54ab9220", "54ab9230", "54ab9310",
        "54ab9320", "54ab9330",
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
async fn test_regions_scenario5_order4_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "54ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "54ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "54ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "54ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "54ac9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "54ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "54ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "54ac9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "54ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "54ac9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "54ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "54ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "54ac9110", "54ac9120", "54ac9130", "54ac9210", "54ac9220", "54ac9230", "54ac9310",
        "54ac9320", "54ac9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3, // we should receive 3 declarations (1 from each of 2 routers within the sub-region, and 1 from the main region)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario5_order4_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "54ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "54ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "54ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "54ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "54ad9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "54ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "54ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "54ad9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
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

    let z9300 = ztimeout!(Node::new(Router, "54ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "54ad9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "54ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "54ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
        "54ad9110", "54ad9120", "54ad9130", "54ad9210", "54ad9220", "54ad9230", "54ad9310",
        "54ad9320", "54ad9330",
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
async fn test_regions_scenario5_order4_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "54ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "54ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "54ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "54ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "54ae9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9110), loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "54ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "54ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "54ae9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9210), loc!(z9220)])
        .open());

    skip_fmt! {
        let s9110 = z9110.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.liveliness().declare_subscriber("test/**").history(true).with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "54ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "54ae9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "54ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "54ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9310), loc!(z9320)])
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
