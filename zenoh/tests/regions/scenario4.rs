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
// C C C  C C C  C C C

use std::time::Duration;

use zenoh_config::WhatAmI::{Client, Router};
use zenoh_core::{lazy_static, ztimeout};

use crate::{count, loc, skip_fmt, Node, SubUtils};

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
async fn test_regions_scenario4_order1_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "41aa9100")
        .endpoints("tcp/[::]:0", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "41aa9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9100)])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "41aa9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9100), loc!(_z9200)])
        .open());

    let _z9110 = ztimeout!(Node::new(Client, "41aa9110")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9120 = ztimeout!(Node::new(Client, "41aa9120")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9130 = ztimeout!(Node::new(Client, "41aa9130")
        .connect(&[loc!(_z9100)])
        .open());

    let _z9210 = ztimeout!(Node::new(Client, "41aa9210")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9220 = ztimeout!(Node::new(Client, "41aa9220")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9230 = ztimeout!(Node::new(Client, "41aa9230")
        .connect(&[loc!(_z9200)])
        .open());

    let _z9310 = ztimeout!(Node::new(Client, "41aa9310")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9320 = ztimeout!(Node::new(Client, "41aa9320")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9330 = ztimeout!(Node::new(Client, "41aa9330")
        .connect(&[loc!(_z9300)])
        .open());

    skip_fmt! {
        let s9110 = _z9110.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9120 = _z9120.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9130 = _z9130.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9210 = _z9210.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9220 = _z9220.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9230 = _z9230.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9310 = _z9310.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9320 = _z9320.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9330 = _z9330.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    ztimeout!(async {
        loop {
            _z9110.put("test/9110", "9110").await.unwrap();
            _z9120.put("test/9120", "9120").await.unwrap();
            _z9130.put("test/9130", "9130").await.unwrap();
            _z9210.put("test/9210", "9210").await.unwrap();
            _z9220.put("test/9220", "9220").await.unwrap();
            _z9230.put("test/9230", "9230").await.unwrap();
            _z9310.put("test/9310", "9310").await.unwrap();
            _z9320.put("test/9320", "9320").await.unwrap();
            _z9330.put("test/9330", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if true
                && s9110.count_keys() == 9
                && s9120.count_keys() == 9
                && s9130.count_keys() == 9
                && s9210.count_keys() == 9
                && s9220.count_keys() == 9
                && s9230.count_keys() == 9
                && s9310.count_keys() == 9
                && s9320.count_keys() == 9
                && s9330.count_keys() == 9
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for i in [
        "41aa9110", "41aa9120", "41aa9130", "41aa9210", "41aa9220", "41aa9230", "41aa9310",
        "41aa9320", "41aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            0
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario4_order1_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "41ab9100")
        .endpoints("tcp/[::]:0", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "41ab9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9100)])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "41ab9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9100), loc!(_z9200)])
        .open());

    let _z9110 = ztimeout!(Node::new(Client, "41ab9110")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9120 = ztimeout!(Node::new(Client, "41ab9120")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9130 = ztimeout!(Node::new(Client, "41ab9130")
        .connect(&[loc!(_z9100)])
        .open());

    let _z9210 = ztimeout!(Node::new(Client, "41ab9210")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9220 = ztimeout!(Node::new(Client, "41ab9220")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9230 = ztimeout!(Node::new(Client, "41ab9230")
        .connect(&[loc!(_z9200)])
        .open());

    let _z9310 = ztimeout!(Node::new(Client, "41ab9310")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9320 = ztimeout!(Node::new(Client, "41ab9320")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9330 = ztimeout!(Node::new(Client, "41ab9330")
        .connect(&[loc!(_z9300)])
        .open());

    skip_fmt! {
        let s9110 = _z9110.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9120 = _z9120.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9130 = _z9130.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9210 = _z9210.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9220 = _z9220.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9230 = _z9230.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9310 = _z9310.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9320 = _z9320.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9330 = _z9330.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    let p9110 = _z9110.declare_publisher("test/9110").await.unwrap();
    let p9120 = _z9120.declare_publisher("test/9120").await.unwrap();
    let p9130 = _z9130.declare_publisher("test/9130").await.unwrap();
    let p9210 = _z9210.declare_publisher("test/9210").await.unwrap();
    let p9220 = _z9220.declare_publisher("test/9220").await.unwrap();
    let p9230 = _z9230.declare_publisher("test/9230").await.unwrap();
    let p9310 = _z9310.declare_publisher("test/9310").await.unwrap();
    let p9320 = _z9320.declare_publisher("test/9320").await.unwrap();
    let p9330 = _z9330.declare_publisher("test/9330").await.unwrap();

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

            if true
                && s9110.count_keys() == 9
                && s9120.count_keys() == 9
                && s9130.count_keys() == 9
                && s9210.count_keys() == 9
                && s9220.count_keys() == 9
                && s9230.count_keys() == 9
                && s9310.count_keys() == 9
                && s9320.count_keys() == 9
                && s9330.count_keys() == 9
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for i in [
        "41ab9110", "41ab9120", "41ab9130", "41ab9210", "41ab9220", "41ab9230", "41ab9310",
        "41ab9320", "41ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            1
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario4_order2_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "42aa9100")
        .endpoints("tcp/[::]:0", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "42aa9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9100)])
        .open());

    let _z9110 = ztimeout!(Node::new(Client, "42aa9110")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9120 = ztimeout!(Node::new(Client, "42aa9120")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9130 = ztimeout!(Node::new(Client, "42aa9130")
        .connect(&[loc!(_z9100)])
        .open());

    let _z9210 = ztimeout!(Node::new(Client, "42aa9210")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9220 = ztimeout!(Node::new(Client, "42aa9220")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9230 = ztimeout!(Node::new(Client, "42aa9230")
        .connect(&[loc!(_z9200)])
        .open());

    skip_fmt! {
        let s9110 = _z9110.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9120 = _z9120.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9130 = _z9130.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9210 = _z9210.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9220 = _z9220.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9230 = _z9230.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    let _z9300 = ztimeout!(Node::new(Router, "42aa9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9100), loc!(_z9200)])
        .open());

    let _z9310 = ztimeout!(Node::new(Client, "42aa9310")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9320 = ztimeout!(Node::new(Client, "42aa9320")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9330 = ztimeout!(Node::new(Client, "42aa9330")
        .connect(&[loc!(_z9300)])
        .open());

    skip_fmt! {
        let s9310 = _z9310.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9320 = _z9320.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9330 = _z9330.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    ztimeout!(async {
        loop {
            _z9110.put("test/9110", "9110").await.unwrap();
            _z9120.put("test/9120", "9120").await.unwrap();
            _z9130.put("test/9130", "9130").await.unwrap();
            _z9210.put("test/9210", "9210").await.unwrap();
            _z9220.put("test/9220", "9220").await.unwrap();
            _z9230.put("test/9230", "9230").await.unwrap();
            _z9310.put("test/9310", "9310").await.unwrap();
            _z9320.put("test/9320", "9320").await.unwrap();
            _z9330.put("test/9330", "9330").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            if true
                && s9110.count_keys() == 9
                && s9120.count_keys() == 9
                && s9130.count_keys() == 9
                && s9210.count_keys() == 9
                && s9220.count_keys() == 9
                && s9230.count_keys() == 9
                && s9310.count_keys() == 9
                && s9320.count_keys() == 9
                && s9330.count_keys() == 9
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for i in [
        "42aa9110", "42aa9120", "42aa9130", "42aa9210", "42aa9220", "42aa9230", "42aa9310",
        "42aa9320", "42aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            0
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario4_order2_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "42ab9100")
        .endpoints("tcp/[::]:0", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "42ab9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9100)])
        .open());

    let _z9110 = ztimeout!(Node::new(Client, "42ab9110")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9120 = ztimeout!(Node::new(Client, "42ab9120")
        .connect(&[loc!(_z9100)])
        .open());
    let _z9130 = ztimeout!(Node::new(Client, "42ab9130")
        .connect(&[loc!(_z9100)])
        .open());

    let _z9210 = ztimeout!(Node::new(Client, "42ab9210")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9220 = ztimeout!(Node::new(Client, "42ab9220")
        .connect(&[loc!(_z9200)])
        .open());
    let _z9230 = ztimeout!(Node::new(Client, "42ab9230")
        .connect(&[loc!(_z9200)])
        .open());

    skip_fmt! {
        let s9110 = _z9110.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9120 = _z9120.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9130 = _z9130.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9210 = _z9210.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9220 = _z9220.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9230 = _z9230.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    let p9110 = _z9110.declare_publisher("test/9110").await.unwrap();
    let p9120 = _z9120.declare_publisher("test/9120").await.unwrap();
    let p9130 = _z9130.declare_publisher("test/9130").await.unwrap();
    let p9210 = _z9210.declare_publisher("test/9210").await.unwrap();
    let p9220 = _z9220.declare_publisher("test/9220").await.unwrap();
    let p9230 = _z9230.declare_publisher("test/9230").await.unwrap();

    let _z9300 = ztimeout!(Node::new(Router, "42ab9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9100), loc!(_z9200)])
        .open());

    let _z9310 = ztimeout!(Node::new(Client, "42ab9310")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9320 = ztimeout!(Node::new(Client, "42ab9320")
        .connect(&[loc!(_z9300)])
        .open());
    let _z9330 = ztimeout!(Node::new(Client, "42ab9330")
        .connect(&[loc!(_z9300)])
        .open());

    skip_fmt! {
        let s9310 = _z9310.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9320 = _z9320.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
        let s9330 = _z9330.declare_subscriber("test/**").with(flume::unbounded()).await.unwrap();
    }

    let p9310 = _z9310.declare_publisher("test/9310").await.unwrap();
    let p9320 = _z9320.declare_publisher("test/9320").await.unwrap();
    let p9330 = _z9330.declare_publisher("test/9330").await.unwrap();

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

            if true
                && s9110.count_keys() == 9
                && s9120.count_keys() == 9
                && s9130.count_keys() == 9
                && s9210.count_keys() == 9
                && s9220.count_keys() == 9
                && s9230.count_keys() == 9
                && s9310.count_keys() == 9
                && s9320.count_keys() == 9
                && s9330.count_keys() == 9
            {
                break;
            }
        }
    });

    let s = STORAGE.lock();

    for i in [
        "42ab9110", "42ab9120", "42ab9130", "42ab9210", "42ab9220", "42ab9230", "42ab9310",
        "42ab9320", "42ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            1
        );
    }
}
