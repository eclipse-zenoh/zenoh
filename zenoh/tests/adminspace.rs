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
#![cfg(feature = "internal_config")]

use std::time::Duration;

use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_wonly() {
    const TIMEOUT: Duration = Duration::from_secs(60);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.adminspace.set_enabled(true).unwrap();
        c.adminspace.permissions.set_read(false).unwrap();
        c.adminspace.permissions.set_write(true).unwrap();
        c.routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid = router.zid();
    let root = router
        .get(format!("@/{zid}/router"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(root.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_read() {
    const TIMEOUT: Duration = Duration::from_secs(60);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:31000";
    const MULTICAST_ENDPOINT: &str = "udp/224.0.0.224:31000";

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen
            .endpoints
            .set(vec![
                ROUTER_ENDPOINT.parse::<EndPoint>().unwrap(),
                MULTICAST_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.adminspace.set_enabled(true).unwrap();
        c.adminspace.permissions.set_read(true).unwrap();
        c.adminspace.permissions.set_write(false).unwrap();
        c.routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid = router.zid();
    let router2 = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid2 = router2.zid();
    let peer = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Peer)).unwrap();
        c.listen
            .endpoints
            .set(vec![MULTICAST_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };

    let root = router
        .get(format!("@/{zid}/router"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(root.is_some());
    let metrics = router
        .get(format!("@/{zid}/router/metrics"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(metrics.is_some());
    let routers_graph = router
        .get(format!("@/{zid}/router/linkstate/routers"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(routers_graph.is_some());
    let peers_graph = router
        .get(format!("@/{zid}/router/linkstate/peers"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(peers_graph.is_some());

    let subscribers: Vec<String> = router
        .get(format!("@/{zid}/router/subscriber/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(subscribers, vec![] as Vec<String>);
    let subscriber = router.declare_subscriber("some/key").await.unwrap();
    let subscribers: Vec<String> = router
        .get(format!("@/{zid}/router/subscriber/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(
        subscribers,
        vec![format!("@/{zid}/router/subscriber/some/key")]
    );
    subscriber.undeclare().await.unwrap();
    let subscribers: Vec<String> = router
        .get(format!("@/{zid}/router/subscriber/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(subscribers, vec![] as Vec<String>);

    let publishers: Vec<String> = router
        .get(format!("@/{zid}/router/publisher/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(publishers, vec![] as Vec<String>);
    let publisher = router.declare_publisher("some/key").await.unwrap();
    let publishers: Vec<String> = router
        .get(format!("@/{zid}/router/publisher/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(
        publishers,
        vec![format!("@/{zid}/router/publisher/some/key")]
    );
    publisher.undeclare().await.unwrap();
    let publishers: Vec<String> = router
        .get(format!("@/{zid}/router/publisher/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(publishers, vec![] as Vec<String>);

    let queryables: Vec<String> = router
        .get(format!("@/{zid}/router/queryable/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(queryables, vec![] as Vec<String>);
    let queryable = router.declare_queryable("some/key").await.unwrap();
    let queryables: Vec<String> = router
        .get(format!("@/{zid}/router/queryable/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(
        queryables,
        vec![format!("@/{zid}/router/queryable/some/key")]
    );
    queryable.undeclare().await.unwrap();
    let queryables: Vec<String> = router
        .get(format!("@/{zid}/router/queryable/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(queryables, vec![] as Vec<String>);

    let queriers: Vec<String> = router
        .get(format!("@/{zid}/router/querier/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(queriers, vec![] as Vec<String>);
    let querier = router.declare_querier("some/key").await.unwrap();
    let queriers: Vec<String> = router
        .get(format!("@/{zid}/router/querier/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(queriers, vec![format!("@/{zid}/router/querier/some/key")]);
    querier.undeclare().await.unwrap();
    let queriers: Vec<String> = router
        .get(format!("@/{zid}/router/querier/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(queriers, vec![] as Vec<String>);

    let routes: Vec<String> = router
        .get(format!("@/{zid}/router/route/successor/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert!(!routes.is_empty());
    let route = router
        .get(format!(
            "@/{zid}/router/route/successor/src/{zid}/dst/{zid2}"
        ))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(route.is_some());

    peer.close().await.unwrap();
    router2.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_ronly() {
    const TIMEOUT: Duration = Duration::from_secs(60);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.adminspace.set_enabled(true).unwrap();
        c.adminspace.permissions.set_read(true).unwrap();
        c.adminspace.permissions.set_write(false).unwrap();
        c.routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid = router.zid();

    router
        .put(format!("@/{zid}/router/config/zid"), "1")
        .await
        .unwrap();
    router
        .delete(format!("@/{zid}/router/config/zid"))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_write() {
    const TIMEOUT: Duration = Duration::from_secs(60);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.adminspace.set_enabled(true).unwrap();
        c.adminspace.permissions.set_read(true).unwrap();
        c.adminspace.permissions.set_write(true).unwrap();
        c.routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid = router.zid();

    router
        .put(format!("@/{zid}/router/config/zid"), "1")
        .await
        .unwrap();
    router
        .delete(format!("@/{zid}/router/config/zid"))
        .await
        .unwrap();
}
