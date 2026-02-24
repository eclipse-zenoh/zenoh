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

// Scenario 1
//          R
//   C      C      C
// P P P  P P P  P P P

use std::time::Duration;

use predicates::Predicate;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    Wait,
};
use zenoh_config::WhatAmI::{Client, Peer, Router};
use zenoh_core::{lazy_static, ztimeout};

use crate::{json, loc, predicates_ext, skip_fmt, Node, SubUtils};

const TIMEOUT: Duration = Duration::from_secs(60);

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
async fn test_regions_scenario1_order1_putsub() {
    init_tracing_subscriber();

    let z9000 = ztimeout!(Node::new(Router, "11aa9000").listen("tcp/0.0.0.0:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "11aa9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.1:9100")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.1:9200")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.1:9300")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "11aa9110")
        .multicast("224.1.1.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "11aa9120")
        .multicast("224.1.1.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "11aa9130")
        .multicast("224.1.1.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "11aa9210")
        .multicast("224.1.1.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "11aa9220")
        .multicast("224.1.1.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "11aa9230")
        .multicast("224.1.1.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "11aa9310")
        .multicast("224.1.1.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "11aa9320")
        .multicast("224.1.1.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "11aa9330")
        .multicast("224.1.1.1:9300")
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
        "11aa9110", "11aa9120", "11aa9130", "11aa9210", "11aa9220", "11aa9230", "11aa9310",
        "11aa9320", "11aa9330",
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
async fn test_regions_scenario1_order1_pubsub() {
    init_tracing_subscriber();

    let z9000 = ztimeout!(Node::new(Router, "11ab9000").listen("tcp/0.0.0.0:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "11ab9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.2:9100")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.2:9200")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .multicast("224.1.1.2:9300")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "11ab9110")
        .multicast("224.1.1.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "11ab9120")
        .multicast("224.1.1.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "11ab9130")
        .multicast("224.1.1.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "11ab9210")
        .multicast("224.1.1.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "11ab9220")
        .multicast("224.1.1.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "11ab9230")
        .multicast("224.1.1.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "11ab9310")
        .multicast("224.1.1.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "11ab9320")
        .multicast("224.1.1.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "11ab9330")
        .multicast("224.1.1.2:9300")
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
        "11ab9110", "11ab9120", "11ab9130", "11ab9210", "11ab9220", "11ab9230", "11ab9310",
        "11ab9320", "11ab9330",
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
async fn test_regions_scenario1_order1_getque() {
    init_tracing_subscriber();

    let z9000 = ztimeout!(Node::new(Router, "11ac9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "11ac9100")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.3:9100")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11ac9200")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.3:9200")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11ac9300")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.3:9300")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "11ac9110")
        .multicast("224.1.1.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "11ac9120")
        .multicast("224.1.1.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "11ac9130")
        .multicast("224.1.1.3:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "11ac9210")
        .multicast("224.1.1.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "11ac9220")
        .multicast("224.1.1.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "11ac9230")
        .multicast("224.1.1.3:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "11ac9310")
        .multicast("224.1.1.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "11ac9320")
        .multicast("224.1.1.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "11ac9330")
        .multicast("224.1.1.3:9300")
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
        "11ac9110", "11ac9120", "11ac9130", "11ac9210", "11ac9220", "11ac9230", "11ac9310",
        "11ac9320", "11ac9330",
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
async fn test_regions_scenario1_order1_queque() {
    init_tracing_subscriber();

    let z9000 = ztimeout!(Node::new(Router, "11ad9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "11ad9100")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.4:9100")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11ad9200")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.4:9200")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11ad9300")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.1.4:9300")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Peer, "11ad9110")
        .multicast("224.1.1.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "11ad9120")
        .multicast("224.1.1.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "11ad9130")
        .multicast("224.1.1.4:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "11ad9210")
        .multicast("224.1.1.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "11ad9220")
        .multicast("224.1.1.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "11ad9230")
        .multicast("224.1.1.4:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "11ad9310")
        .multicast("224.1.1.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "11ad9320")
        .multicast("224.1.1.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "11ad9330")
        .multicast("224.1.1.4:9300")
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
        "11ad9110", "11ad9120", "11ad9130", "11ad9210", "11ad9220", "11ad9230", "11ad9310",
        "11ad9320", "11ad9330",
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
async fn test_regions_scenario1_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "12aa9110")
        .multicast("224.1.2.1:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "12aa9120")
        .multicast("224.1.2.1:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "12aa9130")
        .multicast("224.1.2.1:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "12aa9210")
        .multicast("224.1.2.1:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "12aa9220")
        .multicast("224.1.2.1:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "12aa9230")
        .multicast("224.1.2.1:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "12aa9310")
        .multicast("224.1.2.1:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "12aa9320")
        .multicast("224.1.2.1:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "12aa9330")
        .multicast("224.1.2.1:9300")
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

    let z9000 = ztimeout!(Node::new(Router, "12aa9000").listen("tcp/0.0.0.0:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12aa9100")
        .multicast("224.1.2.1:9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12aa9200")
        .multicast("224.1.2.1:9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12aa9300")
        .multicast("224.1.2.1:9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
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
        "12aa9110", "12aa9120", "12aa9130", "12aa9210", "12aa9220", "12aa9230", "12aa9310",
        "12aa9320", "12aa9330",
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
async fn test_regions_scenario1_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "12ab9110")
        .multicast("224.1.2.2:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "12ab9120")
        .multicast("224.1.2.2:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "12ab9130")
        .multicast("224.1.2.2:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "12ab9210")
        .multicast("224.1.2.2:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "12ab9220")
        .multicast("224.1.2.2:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "12ab9230")
        .multicast("224.1.2.2:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "12ab9310")
        .multicast("224.1.2.2:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "12ab9320")
        .multicast("224.1.2.2:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "12ab9330")
        .multicast("224.1.2.2:9300")
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

    let z9000 = ztimeout!(Node::new(Router, "12ab9000").listen("tcp/0.0.0.0:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12ab9100")
        .multicast("224.1.2.2:9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12ab9200")
        .multicast("224.1.2.2:9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12ab9300")
        .multicast("224.1.2.2:9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
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
        "12ab9110", "12ab9120", "12ab9130", "12ab9210", "12ab9220", "12ab9230", "12ab9310",
        "12ab9320", "12ab9330",
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
async fn test_regions_scenario1_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "12ac9110")
        .multicast("224.1.2.3:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "12ac9120")
        .multicast("224.1.2.3:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "12ac9130")
        .multicast("224.1.2.3:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "12ac9210")
        .multicast("224.1.2.3:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "12ac9220")
        .multicast("224.1.2.3:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "12ac9230")
        .multicast("224.1.2.3:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "12ac9310")
        .multicast("224.1.2.3:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "12ac9320")
        .multicast("224.1.2.3:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "12ac9330")
        .multicast("224.1.2.3:9300")
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

    let z9000 = ztimeout!(Node::new(Router, "12ac9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12ac9100")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.3:9100")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12ac9200")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.3:9200")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12ac9300")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.3:9300")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
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
        "12ac9110", "12ac9120", "12ac9130", "12ac9210", "12ac9220", "12ac9230", "12ac9310",
        "12ac9320", "12ac9330",
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
async fn test_regions_scenario1_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Peer, "12ad9110")
        .multicast("224.1.2.4:9100")
        .open());
    let z9120 = ztimeout!(Node::new(Peer, "12ad9120")
        .multicast("224.1.2.4:9100")
        .open());
    let z9130 = ztimeout!(Node::new(Peer, "12ad9130")
        .multicast("224.1.2.4:9100")
        .open());

    let z9210 = ztimeout!(Node::new(Peer, "12ad9210")
        .multicast("224.1.2.4:9200")
        .open());
    let z9220 = ztimeout!(Node::new(Peer, "12ad9220")
        .multicast("224.1.2.4:9200")
        .open());
    let z9230 = ztimeout!(Node::new(Peer, "12ad9230")
        .multicast("224.1.2.4:9200")
        .open());

    let z9310 = ztimeout!(Node::new(Peer, "12ad9310")
        .multicast("224.1.2.4:9300")
        .open());
    let z9320 = ztimeout!(Node::new(Peer, "12ad9320")
        .multicast("224.1.2.4:9300")
        .open());
    let z9330 = ztimeout!(Node::new(Peer, "12ad9330")
        .multicast("224.1.2.4:9300")
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

    let z9000 = ztimeout!(Node::new(Router, "12ad9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12ad9100")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.4:9100")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12ad9200")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.4:9200")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12ad9300")
        .endpoints("tcp/[::]:0", &[loc!(z9000)])
        .multicast("224.1.2.4:9300")
        .gateway("{south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
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
        "12ad9110", "12ad9120", "12ad9130", "12ad9210", "12ad9220", "12ad9230", "12ad9310",
        "12ad9320", "12ad9330",
    ] {
        assert_eq!(
            s.all_events()
                .filter(|e| predicates_ext::register_queryable(zid, "test").eval(e))
                .count(),
            3
        );
    }
}
