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

    let tokens: Vec<String> = router
        .get(format!("@/{zid}/router/token/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(tokens, vec![] as Vec<String>);
    let token = router.liveliness().declare_token("some/key").await.unwrap();
    let tokens: Vec<String> = router
        .get(format!("@/{zid}/router/token/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(tokens, vec![format!("@/{zid}/router/token/some/key")]);
    token.undeclare().await.unwrap();
    let tokens: Vec<String> = router
        .get(format!("@/{zid}/router/token/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert_eq!(tokens, vec![] as Vec<String>);

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

    let count = router.get("@/**").await.unwrap().iter().count();
    assert!(count > 0);

    let count = router.get("@/*/**").await.unwrap().iter().count();
    assert!(count > 0);

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

// Helper function to assert JSON field exists
fn assert_json_field_exists(json: &serde_json::Value, path: &str, field_name: &str) {
    assert!(
        json.get(path).is_some(),
        "JSON should have '{}' field",
        field_name
    );
}

// Helper function to assert JSON string field value
fn assert_json_string(json: &serde_json::Value, path: &str, expected: &str, description: &str) {
    assert_eq!(
        json[path].as_str().unwrap(),
        expected,
        "{}",
        description
    );
}

// Helper function to assert JSON field is a boolean
fn assert_json_bool_type(json: &serde_json::Value, path: &str, description: &str) {
    assert!(json[path].is_boolean(), "{}", description);
}

// Helper function to assert JSON field is a number and optionally validate it
fn assert_json_number(
    json: &serde_json::Value,
    path: &str,
    validator: impl Fn(u64) -> bool,
    description: &str,
) {
    let value = json[path].as_u64().unwrap();
    assert!(validator(value), "{}, got: {}", description, value);
}

// Helper function to assert JSON string contains substring
#[allow(dead_code)]
fn assert_json_string_contains(
    json: &serde_json::Value,
    path: &str,
    substring: &str,
    description: &str,
) {
    let value = json[path].as_str().unwrap();
    assert!(
        value.contains(substring),
        "{}, got: {}",
        description,
        value
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_transports_and_links() {
    const TIMEOUT: Duration = Duration::from_secs(60);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:31001";

    zenoh_util::init_log_from_env_or("error");

    // Create router1 with adminspace enabled
    let router1 = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
    let zid1 = router1.zid();

    // Test 1: Query transports when none exist (except self-connections)
    let transports_unicast: Vec<String> = router1
        .get(format!("@/{zid1}/session/transport/unicast/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    // Initially, there should be no unicast transports to other peers
    assert_eq!(transports_unicast, vec![] as Vec<String>);

    // Create router2 that connects to router1 (creates unicast transport)
    let router2 = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let s = ztimeout!(zenoh::open(c)).unwrap();
        s
    };
    let zid2 = router2.zid();

    // Give some time for the connection to establish
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test 2: Query all unicast transports - should now find router2
    let transports_unicast: Vec<String> = router1
        .get(format!("@/{zid1}/session/transport/unicast/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert!(!transports_unicast.is_empty());
    assert!(transports_unicast
        .iter()
        .any(|k| k.contains(&zid2.to_string())));

    // Test 3: Query specific unicast transport and parse JSON
    let transport_reply = router1
        .get(format!("@/{zid1}/session/transport/unicast/{zid2}"))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(transport_reply.is_some());
    let binding = transport_reply.unwrap();
    let transport_sample = binding.result().ok().unwrap();
    assert_eq!(
        transport_sample.key_expr().as_str(),
        format!("@/{zid1}/session/transport/unicast/{zid2}")
    );
    // Verify it's JSON encoded
    assert_eq!(
        transport_sample.encoding(),
        &zenoh::bytes::Encoding::APPLICATION_JSON
    );

    // Parse and verify JSON content
    let transport_bytes = transport_sample.payload().to_bytes();
    let transport_json: serde_json::Value =
        serde_json::from_slice(&transport_bytes).expect("Failed to parse transport JSON");

    // Verify TransportPeer fields using helper functions
    assert_json_string(
        &transport_json,
        "zid",
        &zid2.to_string(),
        "Transport ZID should match router2's ZID",
    );
    assert_json_field_exists(&transport_json, "whatami", "whatami");
    assert_json_string(
        &transport_json,
        "whatami",
        "router",
        "Transport whatami should be 'router'",
    );
    assert_json_field_exists(&transport_json, "is_qos", "is_qos");
    assert_json_bool_type(&transport_json, "is_qos", "is_qos should be a boolean value");

    // Test 4: Query links for the unicast transport
    let links: Vec<String> = router1
        .get(format!("@/{zid1}/session/transport/unicast/{zid2}/link/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert!(!links.is_empty());
    // Extract link_id from the first link key expression
    let link_key = &links[0];
    let link_id = link_key.split('/').last().unwrap();

    // Test 5: Query specific link and parse JSON
    let link_reply = router1
        .get(format!(
            "@/{zid1}/session/transport/unicast/{zid2}/link/{link_id}"
        ))
        .await
        .unwrap()
        .into_iter()
        .next();
    assert!(link_reply.is_some());
    let binding = link_reply.unwrap();
    let link_sample = binding.result().ok().unwrap();
    assert_eq!(
        link_sample.encoding(),
        &zenoh::bytes::Encoding::APPLICATION_JSON
    );

    // Parse and verify link JSON content
    let link_bytes = link_sample.payload().to_bytes();
    let link_json: serde_json::Value =
        serde_json::from_slice(&link_bytes).expect("Failed to parse link JSON");

    // Verify Link fields using helper functions
    assert_json_field_exists(&link_json, "src", "src (source locator)");
    assert_json_field_exists(&link_json, "dst", "dst (destination locator)");
    assert_json_field_exists(&link_json, "mtu", "mtu");
    assert_json_field_exists(&link_json, "is_streamed", "is_streamed");

    // Verify src and dst are valid locator strings
    let src_str = link_json["src"].as_str().unwrap();
    let dst_str = link_json["dst"].as_str().unwrap();
    assert!(
        src_str.contains("tcp/") || src_str.contains("localhost"),
        "Source locator should be a valid TCP address, got: {}",
        src_str
    );
    assert!(
        dst_str.contains("tcp/") || dst_str.contains("localhost"),
        "Destination locator should be a valid TCP address, got: {}",
        dst_str
    );

    // Verify MTU is a positive number using helper
    assert_json_number(&link_json, "mtu", |v| v > 0, "MTU should be positive");

    // For TCP links, is_streamed should typically be true
    if src_str.contains("tcp/") {
        assert_eq!(
            link_json["is_streamed"].as_bool().unwrap(),
            true,
            "TCP links should be streamed"
        );
    }

    // Test 6: Verify transport query with wildcard works for all transports
    let all_transports: Vec<String> = router1
        .get(format!("@/{zid1}/session/**"))
        .await
        .unwrap()
        .iter()
        .map(|r| r.result().ok().unwrap().key_expr().to_string())
        .collect();
    assert!(all_transports
        .iter()
        .any(|k| k.contains("transport/unicast") && k.contains(&zid2.to_string())));

    // Cleanup
    router2.close().await.unwrap();
    router1.close().await.unwrap();
}

// Note: Subscription tests for transport and link events are not included here
// because the adminspace uses internal subscriber callbacks (execute_subscriber_callbacks)
// rather than the public zenoh subscription API. The query tests above verify that
// transport and link information can be queried successfully.
