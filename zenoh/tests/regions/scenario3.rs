//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made availbble under the
// terms of the Eclipse Public License 2.0 which is availbble at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is availbble at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

// Scenario 3
//   R      R      R
// P P P  P P P  P P P

use std::time::Duration;

use predicates::Predicate;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    Wait,
};
use zenoh_config::WhatAmI::{Peer, Router};
use zenoh_core::{lazy_static, ztimeout};

use crate::{loc, predicates_ext, skip_fmt, Node, SubUtils};

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
async fn test_regions_scenario3_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "31aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.1.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "31aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.1.1:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.1.1:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "31aa9110")
        .multicast("224.3.1.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "31aa9120")
        .multicast("224.3.1.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "31aa9130")
        .multicast("224.3.1.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "31aa9210")
        .multicast("224.3.1.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "31aa9220")
        .multicast("224.3.1.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "31aa9230")
        .multicast("224.3.1.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "31aa9310")
        .multicast("224.3.1.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "31aa9320")
        .multicast("224.3.1.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "31aa9330")
        .multicast("224.3.1.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "31aa9110", "31aa9120", "31aa9130", "31aa9210", "31aa9220", "31aa9230", "31aa9310",
        "31aa9320", "31aa9330",
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
async fn test_regions_scenario3_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "31ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.1.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "31ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.1.2:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.1.2:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "31ab9110")
        .multicast("224.3.1.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "31ab9120")
        .multicast("224.3.1.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "31ab9130")
        .multicast("224.3.1.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "31ab9210")
        .multicast("224.3.1.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "31ab9220")
        .multicast("224.3.1.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "31ab9230")
        .multicast("224.3.1.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "31ab9310")
        .multicast("224.3.1.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "31ab9320")
        .multicast("224.3.1.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "31ab9330")
        .multicast("224.3.1.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "31ab9110", "31ab9120", "31ab9130", "31ab9210", "31ab9220", "31ab9230", "31ab9310",
        "31ab9320", "31ab9330",
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
async fn test_regions_scenario3_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "31ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.1.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "31ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.1.3:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.1.3:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "31ac9110")
        .multicast("224.3.1.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "31ac9120")
        .multicast("224.3.1.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "31ac9130")
        .multicast("224.3.1.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "31ac9210")
        .multicast("224.3.1.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "31ac9220")
        .multicast("224.3.1.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "31ac9230")
        .multicast("224.3.1.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "31ac9310")
        .multicast("224.3.1.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "31ac9320")
        .multicast("224.3.1.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "31ac9330")
        .multicast("224.3.1.3:9300")
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
        "31ac9110", "31ac9120", "31ac9130", "31ac9210", "31ac9220", "31ac9230", "31ac9310",
        "31ac9320", "31ac9330",
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
async fn test_regions_scenario3_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "31ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.1.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "31ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.1.4:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.1.4:9300")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "31ad9110")
        .multicast("224.3.1.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "31ad9120")
        .multicast("224.3.1.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "31ad9130")
        .multicast("224.3.1.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "31ad9210")
        .multicast("224.3.1.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "31ad9220")
        .multicast("224.3.1.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "31ad9230")
        .multicast("224.3.1.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "31ad9310")
        .multicast("224.3.1.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "31ad9320")
        .multicast("224.3.1.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "31ad9330")
        .multicast("224.3.1.4:9300")
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
        "31ad9110", "31ad9120", "31ad9130", "31ad9210", "31ad9220", "31ad9230", "31ad9310",
        "31ad9320", "31ad9330",
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
async fn test_regions_scenario3_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "32aa9110")
        .multicast("224.3.2.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "32aa9120")
        .multicast("224.3.2.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "32aa9130")
        .multicast("224.3.2.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "32aa9210")
        .multicast("224.3.2.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "32aa9220")
        .multicast("224.3.2.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "32aa9230")
        .multicast("224.3.2.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "32aa9310")
        .multicast("224.3.2.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "32aa9320")
        .multicast("224.3.2.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "32aa9330")
        .multicast("224.3.2.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
    }

    let z9100 = ztimeout!(Node::new(Router, "32aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.2.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "32aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.2.1:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.2.1:9300")
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "32aa9110", "32aa9120", "32aa9130", "32aa9210", "32aa9220", "32aa9230", "32aa9310",
        "32aa9320", "32aa9330",
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
async fn test_regions_scenario3_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "32ab9110")
        .multicast("224.3.2.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "32ab9120")
        .multicast("224.3.2.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "32ab9130")
        .multicast("224.3.2.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "32ab9210")
        .multicast("224.3.2.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "32ab9220")
        .multicast("224.3.2.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "32ab9230")
        .multicast("224.3.2.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "32ab9310")
        .multicast("224.3.2.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "32ab9320")
        .multicast("224.3.2.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "32ab9330")
        .multicast("224.3.2.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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

    let z9100 = ztimeout!(Node::new(Router, "32ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.2.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "32ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.2.2:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.2.2:9300")
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "32ab9110", "32ab9120", "32ab9130", "32ab9210", "32ab9220", "32ab9230", "32ab9310",
        "32ab9320", "32ab9330",
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
async fn test_regions_scenario3_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "32ac9110")
        .multicast("224.3.2.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "32ac9120")
        .multicast("224.3.2.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "32ac9130")
        .multicast("224.3.2.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "32ac9210")
        .multicast("224.3.2.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "32ac9220")
        .multicast("224.3.2.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "32ac9230")
        .multicast("224.3.2.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "32ac9310")
        .multicast("224.3.2.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "32ac9320")
        .multicast("224.3.2.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "32ac9330")
        .multicast("224.3.2.3:9300")
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

    let z9100 = ztimeout!(Node::new(Router, "32ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.2.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "32ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.2.3:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.2.3:9300")
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
        "32ac9110", "32ac9120", "32ac9130", "32ac9210", "32ac9220", "32ac9230", "32ac9310",
        "32ac9320", "32ac9330",
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
async fn test_regions_scenario3_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "32ad9110")
        .multicast("224.3.2.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "32ad9120")
        .multicast("224.3.2.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "32ad9130")
        .multicast("224.3.2.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "32ad9210")
        .multicast("224.3.2.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "32ad9220")
        .multicast("224.3.2.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "32ad9230")
        .multicast("224.3.2.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "32ad9310")
        .multicast("224.3.2.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "32ad9320")
        .multicast("224.3.2.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "32ad9330")
        .multicast("224.3.2.4:9300")
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

    let z9100 = ztimeout!(Node::new(Router, "32ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.2.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "32ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.2.4:9200")
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.2.4:9300")
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
        "32ad9110", "32ad9120", "32ad9130", "32ad9210", "32ad9220", "32ad9230", "32ad9310",
        "32ad9320", "32ad9330",
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
async fn test_regions_scenario3_order3_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "33aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.3.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "33aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.3.1:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "33aa9110")
        .multicast("224.3.3.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "33aa9120")
        .multicast("224.3.3.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "33aa9130")
        .multicast("224.3.3.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "33aa9210")
        .multicast("224.3.3.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "33aa9220")
        .multicast("224.3.3.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "33aa9230")
        .multicast("224.3.3.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "33aa9310")
        .multicast("224.3.3.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "33aa9320")
        .multicast("224.3.3.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "33aa9330")
        .multicast("224.3.3.1:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Router, "33aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.3.1:9300")
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "33aa9110", "33aa9120", "33aa9130", "33aa9210", "33aa9220", "33aa9230", "33aa9310",
        "33aa9320", "33aa9330",
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
async fn test_regions_scenario3_order3_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "33ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.3.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "33ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.3.2:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "33ab9110")
        .multicast("224.3.3.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "33ab9120")
        .multicast("224.3.3.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "33ab9130")
        .multicast("224.3.3.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "33ab9210")
        .multicast("224.3.3.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "33ab9220")
        .multicast("224.3.3.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "33ab9230")
        .multicast("224.3.3.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "33ab9310")
        .multicast("224.3.3.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "33ab9320")
        .multicast("224.3.3.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "33ab9330")
        .multicast("224.3.3.2:9300")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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

    let _z9300 = ztimeout!(Node::new(Router, "33ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.3.2:9300")
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "33ab9110", "33ab9120", "33ab9130", "33ab9210", "33ab9220", "33ab9230", "33ab9310",
        "33ab9320", "33ab9330",
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
async fn test_regions_scenario3_order3_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "33ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.3.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "33ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.3.3:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "33ac9110")
        .multicast("224.3.3.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "33ac9120")
        .multicast("224.3.3.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "33ac9130")
        .multicast("224.3.3.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "33ac9210")
        .multicast("224.3.3.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "33ac9220")
        .multicast("224.3.3.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "33ac9230")
        .multicast("224.3.3.3:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "33ac9310")
        .multicast("224.3.3.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "33ac9320")
        .multicast("224.3.3.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "33ac9330")
        .multicast("224.3.3.3:9300")
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

    let _z9300 = ztimeout!(Node::new(Router, "33ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.3.3:9300")
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
        "33ac9110", "33ac9120", "33ac9130", "33ac9210", "33ac9220", "33ac9230", "33ac9310",
        "33ac9320", "33ac9330",
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
async fn test_regions_scenario3_order3_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "33ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.3.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "33ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.3.4:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "33ad9110")
        .multicast("224.3.3.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "33ad9120")
        .multicast("224.3.3.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "33ad9130")
        .multicast("224.3.3.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "33ad9210")
        .multicast("224.3.3.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "33ad9220")
        .multicast("224.3.3.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "33ad9230")
        .multicast("224.3.3.4:9200")
        .open());
    let z9310 = ztimeout!(Node::new(Peer, "33ad9310")
        .multicast("224.3.3.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "33ad9320")
        .multicast("224.3.3.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "33ad9330")
        .multicast("224.3.3.4:9300")
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

    let _z9300 = ztimeout!(Node::new(Router, "33ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.3.4:9300")
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
        "33ad9110", "33ad9120", "33ad9130", "33ad9210", "33ad9220", "33ad9230", "33ad9310",
        "33ad9320", "33ad9330",
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
async fn test_regions_scenario3_order4_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "34aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.4.1:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "34aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.4.1:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "34aa9110")
        .multicast("224.3.4.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "34aa9120")
        .multicast("224.3.4.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "34aa9130")
        .multicast("224.3.4.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "34aa9210")
        .multicast("224.3.4.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "34aa9220")
        .multicast("224.3.4.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "34aa9230")
        .multicast("224.3.4.1:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Router, "34aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.4.1:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "34aa9310")
        .multicast("224.3.4.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "34aa9320")
        .multicast("224.3.4.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "34aa9330")
        .multicast("224.3.4.1:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "34aa9110", "34aa9120", "34aa9130", "34aa9210", "34aa9220", "34aa9230", "34aa9310",
        "34aa9320", "34aa9330",
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
async fn test_regions_scenario3_order4_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "34ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.4.2:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "34ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.4.2:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "34ab9110")
        .multicast("224.3.4.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "34ab9120")
        .multicast("224.3.4.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "34ab9130")
        .multicast("224.3.4.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "34ab9210")
        .multicast("224.3.4.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "34ab9220")
        .multicast("224.3.4.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "34ab9230")
        .multicast("224.3.4.2:9200")
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
    }

    let p9110 = z9110.declare_publisher("test").await.unwrap();
    let p9120 = z9120.declare_publisher("test").await.unwrap();
    let p9130 = z9130.declare_publisher("test").await.unwrap();
    let p9210 = z9210.declare_publisher("test").await.unwrap();
    let p9220 = z9220.declare_publisher("test").await.unwrap();
    let p9230 = z9230.declare_publisher("test").await.unwrap();

    let _z9300 = ztimeout!(Node::new(Router, "34ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.4.2:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "34ab9310")
        .multicast("224.3.4.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "34ab9320")
        .multicast("224.3.4.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "34ab9330")
        .multicast("224.3.4.2:9300")
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(flume::unbounded()).await.unwrap();
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
            .all(|sub| sub.count_vals() == 9)
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for zid in [
        "34ab9110", "34ab9120", "34ab9130", "34ab9210", "34ab9220", "34ab9230", "34ab9310",
        "34ab9320", "34ab9330",
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
async fn test_regions_scenario3_order4_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "34ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.4.3:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "34ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.4.3:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "34ac9110")
        .multicast("224.3.4.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "34ac9120")
        .multicast("224.3.4.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "34ac9130")
        .multicast("224.3.4.3:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "34ac9210")
        .multicast("224.3.4.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "34ac9220")
        .multicast("224.3.4.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "34ac9230")
        .multicast("224.3.4.3:9200")
        .open());

    skip_fmt! {
        let _q9110 = z9110.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9110")).unwrap()).await.unwrap();
        let _q9120 = z9120.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9120")).unwrap()).await.unwrap();
        let _q9130 = z9130.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9130")).unwrap()).await.unwrap();
        let _q9210 = z9210.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9210")).unwrap()).await.unwrap();
        let _q9220 = z9220.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9220")).unwrap()).await.unwrap();
        let _q9230 = z9230.declare_queryable("test").callback(|q| Wait::wait(q.reply("test", "9230")).unwrap()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Router, "34ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.4.3:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "34ac9310")
        .multicast("224.3.4.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "34ac9320")
        .multicast("224.3.4.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "34ac9330")
        .multicast("224.3.4.3:9300")
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
        "34ac9110", "34ac9120", "34ac9130", "34ac9210", "34ac9220", "34ac9230", "34ac9310",
        "34ac9320", "34ac9330",
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
async fn test_regions_scenario3_order4_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "34ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .multicast("224.3.4.4:9100")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "34ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .multicast("224.3.4.4:9200")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "34ad9110")
        .multicast("224.3.4.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "34ad9120")
        .multicast("224.3.4.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "34ad9130")
        .multicast("224.3.4.4:9100")
        .open());
    let z9210 = ztimeout!(Node::new(Peer, "34ad9210")
        .multicast("224.3.4.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "34ad9220")
        .multicast("224.3.4.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "34ad9230")
        .multicast("224.3.4.4:9200")
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

    let _z9300 = ztimeout!(Node::new(Router, "34ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .multicast("224.3.4.4:9300")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "34ad9310")
        .multicast("224.3.4.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "34ad9320")
        .multicast("224.3.4.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "34ad9330")
        .multicast("224.3.4.4:9300")
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
        "34ad9110", "34ad9120", "34ad9130", "34ad9210", "34ad9220", "34ad9230", "34ad9310",
        "34ad9320", "34ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3
        );
    }
}
