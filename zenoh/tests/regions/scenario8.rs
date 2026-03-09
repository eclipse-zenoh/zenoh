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

// Scenario 8
//  R R    R R    R R
// R R R  R R R  R R R

use std::{collections::HashSet, time::Duration};

use futures::StreamExt;
use zenoh::{
    query::{ConsolidationMode, QueryTarget},
    sample::SampleKind,
    Wait,
};
use zenoh_config::WhatAmI::Router;
use zenoh_core::{lazy_static, ztimeout};

use crate::{loc, skip_fmt, unbounded_sink, Node};

const TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    static ref STORAGE: tracing_capture::SharedStorage = tracing_capture::SharedStorage::default();
}

fn init_tracing_subscriber() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("debug,zenoh::net::routing::dispatcher=trace")
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();
}

// NOTE(regions): linkstate updates occurring after entity declarations leads to tree re-computation
// and to re-declarations by consequence. This makes counting registration events unreliable and
// questionable because tree computation is an implementation detail. For this reason, this scenario
// has no assertions on `register_subscriber`/`register_queryable` event counts.

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order1_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "81aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "81aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "81aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "81aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "81aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "81aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "81aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "81aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "81aa9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "81aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "81aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "81aa9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "81aa9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "81aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "81aa9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order1_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "81ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "81ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "81ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "81ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "81ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "81ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "81ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "81ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "81ab9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "81ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "81ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "81ab9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "81ab9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "81ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "81ab9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order1_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "81ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "81ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "81ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "81ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "81ac9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "81ac9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "81ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "81ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "81ac9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "81ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "81ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "81ac9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "81ac9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "81ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "81ac9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order1_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "81ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "81ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "81ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "81ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "81ad9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "81ad9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "81ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "81ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "81ad9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "81ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "81ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "81ad9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "81ad9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "81ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "81ad9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order1_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "81ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "81ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "81ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "81ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "81ae9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "81ae9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "81ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "81ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "81ae9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "81ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "81ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "81ae9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "81ae9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "81ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "81ae9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
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
async fn test_regions_scenario8_order2_putsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "82aa9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "82aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "82aa9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "82aa9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "82aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "82aa9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "82aa9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "82aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "82aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "82aa9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "82aa9101")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9110), loc!(z9120), loc!(z9130)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "82aa9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "82aa9201")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9200),
                loc!(z9100),
                loc!(z9210),
                loc!(z9220),
                loc!(z9230)
            ]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "82aa9300")
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
    let z9301 = ztimeout!(Node::new(Router, "82aa9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order2_pubsub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "82ab9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "82ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "82ab9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "82ab9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "82ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "82ab9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "82ab9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "82ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "82ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "82ab9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "82ab9101")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9110), loc!(z9120), loc!(z9130)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "82ab9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "82ab9201")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9200),
                loc!(z9100),
                loc!(z9210),
                loc!(z9220),
                loc!(z9230)
            ]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "82ab9300")
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
    let z9301 = ztimeout!(Node::new(Router, "82ab9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order2_getque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "82ac9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "82ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "82ac9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9120)])
        .open());
    let z9210 = ztimeout!(Node::new(Router, "82ac9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "82ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "82ac9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9220)])
        .open());
    let z9310 = ztimeout!(Node::new(Router, "82ac9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "82ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "82ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "82ac9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "82ac9101")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9110), loc!(z9120), loc!(z9130)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "82ac9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "82ac9201")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9200),
                loc!(z9100),
                loc!(z9210),
                loc!(z9220),
                loc!(z9230)
            ]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "82ac9300")
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
    let z9301 = ztimeout!(Node::new(Router, "82ac9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order2_queque() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "82ad9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "82ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "82ad9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9120)])
        .open());
    let z9210 = ztimeout!(Node::new(Router, "82ad9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "82ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "82ad9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9220)])
        .open());
    let z9310 = ztimeout!(Node::new(Router, "82ad9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "82ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "82ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "82ad9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "82ad9101")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9110), loc!(z9120), loc!(z9130)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "82ad9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "82ad9201")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9200),
                loc!(z9100),
                loc!(z9210),
                loc!(z9220),
                loc!(z9230)
            ]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "82ad9300")
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
    let z9301 = ztimeout!(Node::new(Router, "82ad9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order2_toksub() {
    init_tracing_subscriber();

    let z9110 = ztimeout!(Node::new(Router, "82ae9110")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "82ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "82ae9130")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9120)])
        .open());

    let z9210 = ztimeout!(Node::new(Router, "82ae9210")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "82ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "82ae9230")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9220)])
        .open());

    let z9310 = ztimeout!(Node::new(Router, "82ae9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "82ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "82ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9100 = ztimeout!(Node::new(Router, "82ae9100")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9110), loc!(z9120), loc!(z9130)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9101 = ztimeout!(Node::new(Router, "82ae9101")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9110), loc!(z9120), loc!(z9130)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "82ae9200")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9210), loc!(z9220), loc!(z9230)]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let _z9201 = ztimeout!(Node::new(Router, "82ae9201")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9200),
                loc!(z9100),
                loc!(z9210),
                loc!(z9220),
                loc!(z9230)
            ]
        )
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9300 = ztimeout!(Node::new(Router, "82ae9300")
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
    let _z9301 = ztimeout!(Node::new(Router, "82ae9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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
async fn test_regions_scenario8_order3_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "83aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "83aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "83aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "83aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "83aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "83aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "83aa9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "83aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "83aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "83aa9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "83aa9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "83aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "83aa9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "83aa9300")
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
    let z9301 = ztimeout!(Node::new(Router, "83aa9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order3_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "83ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "83ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "83ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "83ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "83ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "83ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "83ab9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "83ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "83ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "83ab9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "83ab9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "83ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "83ab9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "83ab9300")
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
    let z9301 = ztimeout!(Node::new(Router, "83ab9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order3_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "83ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "83ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "83ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "83ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "83ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "83ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "83ac9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "83ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "83ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "83ac9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "83ac9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "83ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "83ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "83ac9300")
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
    let z9301 = ztimeout!(Node::new(Router, "83ac9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order3_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "83ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "83ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "83ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "83ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "83ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "83ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "83ad9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "83ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "83ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "83ad9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "83ad9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "83ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "83ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "83ad9300")
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
    let z9301 = ztimeout!(Node::new(Router, "83ad9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order3_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "83ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "83ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "83ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "83ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "83ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "83ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "83ae9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "83ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "83ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "83ae9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "83ae9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "83ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "83ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "83ae9300")
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
    let _z9301 = ztimeout!(Node::new(Router, "83ae9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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
async fn test_regions_scenario8_order4_putsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "84aa9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "84aa9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "84aa9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "84aa9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "84aa9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "84aa9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "84aa9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "84aa9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "84aa9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "84aa9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    skip_fmt! {
        let s9110 = z9110.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9120 = z9120.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9130 = z9130.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9210 = z9210.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9220 = z9220.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9230 = z9230.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let z9300 = ztimeout!(Node::new(Router, "84aa9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "84aa9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "84aa9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "84aa9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "84aa9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order4_pubsub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "84ab9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "84ab9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "84ab9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "84ab9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "84ab9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "84ab9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "84ab9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "84ab9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "84ab9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "84ab9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
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

    let z9300 = ztimeout!(Node::new(Router, "84ab9300")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9301 = ztimeout!(Node::new(Router, "84ab9301")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9100), loc!(z9200)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9310 = ztimeout!(Node::new(Router, "84ab9310")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301)])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "84ab9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9300), loc!(z9301), loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "84ab9330")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9300), loc!(z9301), loc!(z9310), loc!(z9320)]
        )
        .open());

    skip_fmt! {
        let s9310 = z9310.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9320 = z9320.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
        let s9330 = z9330.declare_subscriber("test").with(unbounded_sink()).await.unwrap();
    }

    let p9310 = z9310.declare_publisher("test").await.unwrap();
    let p9320 = z9320.declare_publisher("test").await.unwrap();
    let p9330 = z9330.declare_publisher("test").await.unwrap();

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order4_getque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "84ac9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "84ac9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "84ac9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "84ac9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "84ac9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "84ac9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "84ac9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "84ac9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "84ac9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "84ac9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "84ac9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "84ac9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "84ac9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "84ac9300")
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
    let z9301 = ztimeout!(Node::new(Router, "84ac9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order4_queque() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "84ad9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "84ad9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "84ad9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "84ad9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "84ad9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "84ad9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "84ad9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());
    let z9210 = ztimeout!(Node::new(Router, "84ad9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "84ad9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "84ad9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());
    let z9310 = ztimeout!(Node::new(Router, "84ad9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "84ad9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "84ad9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "84ad9300")
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
    let z9301 = ztimeout!(Node::new(Router, "84ad9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

    for s in [&z9110, &z9120, &z9130] {
        wait_for_routers(s, &[&z9100, &z9101]).await
    }

    for s in [&z9210, &z9220, &z9230] {
        wait_for_routers(s, &[&z9200, &z9201]).await
    }

    for s in [&z9310, &z9320, &z9330] {
        wait_for_routers(s, &[&z9300, &z9301]).await
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario8_order4_toksub() {
    init_tracing_subscriber();

    let z9100 = ztimeout!(Node::new(Router, "84ae9100")
        .endpoints("tcp/0.0.0.0:0", &[])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9101 = ztimeout!(Node::new(Router, "84ae9101")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9200 = ztimeout!(Node::new(Router, "84ae9200")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());
    let z9201 = ztimeout!(Node::new(Router, "84ae9201")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9100)])
        .region("main")
        .gateway("{south:[{filters:[{negated:true,region_names:[\"main\"]}]}]}")
        .open());

    let z9110 = ztimeout!(Node::new(Router, "84ae9110")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101)])
        .open());
    let z9120 = ztimeout!(Node::new(Router, "84ae9120")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9100), loc!(z9101), loc!(z9110)])
        .open());
    let z9130 = ztimeout!(Node::new(Router, "84ae9130")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9100), loc!(z9101), loc!(z9110), loc!(z9120)]
        )
        .open());

    let z9210 = ztimeout!(Node::new(Router, "84ae9210")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201)])
        .open());
    let z9220 = ztimeout!(Node::new(Router, "84ae9220")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9200), loc!(z9201), loc!(z9210)])
        .open());
    let z9230 = ztimeout!(Node::new(Router, "84ae9230")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[loc!(z9200), loc!(z9201), loc!(z9210), loc!(z9220)]
        )
        .open());

    let z9310 = ztimeout!(Node::new(Router, "84ae9310")
        .endpoints("tcp/0.0.0.0:0", &[])
        .open());
    let z9320 = ztimeout!(Node::new(Router, "84ae9320")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9310)])
        .open());
    let z9330 = ztimeout!(Node::new(Router, "84ae9330")
        .endpoints("tcp/0.0.0.0:0", &[loc!(z9320)])
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

    let z9300 = ztimeout!(Node::new(Router, "84ae9300")
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
    let _z9301 = ztimeout!(Node::new(Router, "84ae9301")
        .endpoints(
            "tcp/0.0.0.0:0",
            &[
                loc!(z9300),
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

#[tracing::instrument(level = "debug", skip(others), ret)]
async fn wait_for_routers(this: &zenoh::Session, others: &[&zenoh::Session]) {
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
        assert!(event.transport().whatami().is_router());

        zids.remove(event.transport().zid());

        if zids.is_empty() {
            break;
        }
    }
}
