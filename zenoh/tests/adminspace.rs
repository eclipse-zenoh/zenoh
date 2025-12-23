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
        ztimeout!(zenoh::open(c)).unwrap()
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
        ztimeout!(zenoh::open(c)).unwrap()
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
        ztimeout!(zenoh::open(c)).unwrap()
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
        ztimeout!(zenoh::open(c)).unwrap()
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
        ztimeout!(zenoh::open(c)).unwrap()
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
        ztimeout!(zenoh::open(c)).unwrap()
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

/// Helper macro to navigate JSON path and return the value at that path
macro_rules! navigate_json_path {
    ($json:expr, $field_path:expr) => {{
        let field_path = $field_path;
        let parts: Vec<&str> = field_path.split('.').collect();
        let mut current = &$json;

        // Navigate to the field
        for (i, part) in parts.iter().enumerate() {
            assert!(
                current.get(part).is_some(),
                "JSON field '{}' does not exist (failed at '{}')",
                field_path,
                parts[..=i].join(".")
            );
            current = &current[part];
        }

        (field_path, current)
    }};
}

/// Macro to assert JSON field equals an exact value
///
/// Supports nested field paths using dot notation (e.g., "priorities.start")
///
/// Usage:
/// - `assert_json_field_eq!(json, "field", "expected")` - String equality
/// - `assert_json_field_eq!(json, "field.nested", 42)` - Number equality
/// - `assert_json_field_eq!(json, "field", true)` - Boolean equality
macro_rules! assert_json_field_eq {
    ($json:expr, $field:expr, $expected:expr) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        let expected_value = serde_json::json!($expected);

        assert_eq!(
            current, &expected_value,
            "JSON field '{}' should equal {:?}, got: {:?}",
            field_path, expected_value, current
        );
    }};
}

/// Macro to assert JSON field properties with automatic error messages
///
/// Supports nested field paths using dot notation (e.g., "priorities.start")
///
/// Usage:
/// - `assert_json_field!(json, "field", bool)` - Check field is boolean type
/// - `assert_json_field!(json, "field.nested", str, |v| v == "expected")` - String validator with path
/// - `assert_json_field!(json, "field", number)` - Check field is number type
/// - `assert_json_field!(json, "field", array)` - Check field is array type
/// - `assert_json_field!(json, "field", object)` - Check field is object type
/// - `assert_json_field!(json, "field", bool, |v| v)` - Boolean validator
/// - `assert_json_field!(json, "field", number, |v| v > 0)` - Number validator
macro_rules! assert_json_field {
    // Check field is boolean type
    ($json:expr, $field:expr, bool) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        assert!(
            current.is_boolean(),
            "JSON field '{}' should be a boolean, got: {:?}",
            field_path,
            current
        );
    }};

    // Check field is str type
    ($json:expr, $field:expr, str) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        assert!(
            current.is_string(),
            "JSON field '{}' should be a string, got: {:?}",
            field_path,
            current
        );
    }};

    // Check field is number type
    ($json:expr, $field:expr, number) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        assert!(
            current.is_number(),
            "JSON field '{}' should be a number, got: {:?}",
            field_path,
            current
        );
    }};

    // Check field is array type
    ($json:expr, $field:expr, array) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        assert!(
            current.is_array(),
            "JSON field '{}' should be an array, got: {:?}",
            field_path,
            current
        );
    }};

    // Check field is object type
    ($json:expr, $field:expr, object) => {{
        let (field_path, current) = navigate_json_path!($json, $field);
        assert!(
            current.is_object(),
            "JSON field '{}' should be an object, got: {:?}",
            field_path,
            current
        );
    }};
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adminspace_transports_and_links() {
    const TIMEOUT: Duration = Duration::from_secs(60);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:31001";
    const ROUTER_CONNECT_ENDPOINT: &str = "tcp/localhost:31001?rel=1;prio=1-7";

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
        // Enable QoS for priorities and reliability support
        c.transport.unicast.qos.set_enabled(true).unwrap();
        ztimeout!(zenoh::open(c)).unwrap()
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

    // Test: Subscribe to transport and link keys to receive notifications
    // Create channels to collect subscription samples
    let (transport_tx, mut transport_rx) = tokio::sync::mpsc::unbounded_channel();
    let (link_tx, mut link_rx) = tokio::sync::mpsc::unbounded_channel();

    // Subscribe to unicast transports only (single * matches one level)
    let transport_subscriber = router1
        .declare_subscriber(format!("@/{zid1}/session/transport/unicast/*"))
        .callback(move |sample| {
            transport_tx.send(sample).unwrap();
        })
        .await
        .unwrap();

    // Subscribe to all links (we'll filter for the specific transport after router2 connects)
    let link_subscriber = router1
        .declare_subscriber(format!("@/{zid1}/session/transport/unicast/**/link/*"))
        .callback(move |sample| {
            link_tx.send(sample).unwrap();
        })
        .await
        .unwrap();

    // Create router2 that connects to router1 (creates unicast transport)
    let router2 = {
        let mut c = zenoh::Config::default();
        c.set_mode(Some(WhatAmI::Router)).unwrap();
        c.listen.endpoints.set(vec![]).unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_CONNECT_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        // Enable QoS for priorities and reliability support
        c.transport.unicast.qos.set_enabled(true).unwrap();
        ztimeout!(zenoh::open(c)).unwrap()
    };
    let zid2 = router2.zid();

    // Give some time for the connection to establish
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test: Verify transport subscription notification
    // Give time for subscription notifications to arrive
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Receive exactly one transport sample
    let transport_sample_sub = transport_rx
        .try_recv()
        .expect("Should receive transport notification");
    assert!(
        transport_rx.try_recv().is_err(),
        "Should receive exactly one transport notification"
    );
    assert_eq!(
        transport_sample_sub.key_expr().as_str(),
        format!("@/{zid1}/session/transport/unicast/{zid2}")
    );
    assert_eq!(
        transport_sample_sub.encoding(),
        &zenoh::bytes::Encoding::APPLICATION_JSON
    );

    // Parse and verify JSON from subscription
    let transport_bytes_sub = transport_sample_sub.payload().to_bytes();
    let transport_json_sub: serde_json::Value = serde_json::from_slice(&transport_bytes_sub)
        .expect("Failed to parse transport JSON from subscription");

    println!(
        "\nTransport JSON from subscription:\n{}",
        serde_json::to_string_pretty(&transport_json_sub).unwrap()
    );

    // Verify transport JSON from subscription
    assert_json_field_eq!(transport_json_sub, "zid", &zid2.to_string());
    assert_json_field_eq!(transport_json_sub, "whatami", "router");
    assert_json_field!(transport_json_sub, "is_qos", bool);
    #[cfg(feature = "shared-memory")]
    assert_json_field!(transport_json_sub, "is_shm", bool);

    // Test: Verify link subscription notification
    // Receive exactly one link sample
    let link_sample_sub = link_rx
        .try_recv()
        .expect("Should receive link notification");
    assert!(
        link_rx.try_recv().is_err(),
        "Should receive exactly one link notification"
    );
    assert!(
        link_sample_sub
            .key_expr()
            .as_str()
            .contains(&format!("@/{zid1}/session/transport/unicast/{zid2}/link/")),
        "Link key expression should contain the expected transport path"
    );
    assert_eq!(
        link_sample_sub.encoding(),
        &zenoh::bytes::Encoding::APPLICATION_JSON
    );

    // Parse and verify link JSON from subscription
    let link_bytes_sub = link_sample_sub.payload().to_bytes();
    let link_json_sub: serde_json::Value = serde_json::from_slice(&link_bytes_sub)
        .expect("Failed to parse link JSON from subscription");

    println!(
        "\nLink JSON from subscription:\n{}",
        serde_json::to_string_pretty(&link_json_sub).unwrap()
    );

    // Verify link JSON from subscription has all required fields
    assert_json_field!(link_json_sub, "src", str);
    assert_json_field!(link_json_sub, "dst", str);
    assert_json_field!(link_json_sub, "mtu", number);
    assert_json_field_eq!(link_json_sub, "is_streamed", true);
    assert_json_field!(link_json_sub, "interfaces", array);
    assert_json_field_eq!(link_json_sub, "priorities.start", "RealTime");
    assert_json_field_eq!(link_json_sub, "priorities.end", "Background");
    assert_json_field_eq!(link_json_sub, "reliability", "Reliable");

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

    // Print TransportPeer JSON
    println!(
        "TransportPeer JSON:\n{}",
        serde_json::to_string_pretty(&transport_json).unwrap()
    );

    // Verify all TransportPeer fields using macro
    assert_json_field_eq!(transport_json, "zid", &zid2.to_string());
    assert_json_field_eq!(transport_json, "whatami", "router");
    assert_json_field!(transport_json, "is_qos", bool);
    #[cfg(feature = "shared-memory")]
    assert_json_field!(transport_json, "is_shm", bool);

    // Note: 'links' field is skipped in serialization (see TransportPeer serde(skip))

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

    // Print Link JSON
    println!(
        "\nLink JSON:\n{}",
        serde_json::to_string_pretty(&link_json).unwrap()
    );
    println!("\nNote: 'priorities' and 'reliability' are set via endpoint metadata (e.g., ?rel=1;prio=1-7)");

    // Verify all Link fields comprehensively
    assert_json_field!(link_json, "src", str);
    assert_json_field!(link_json, "dst", str);
    assert_json_field!(link_json, "mtu", number);
    assert_json_field_eq!(link_json, "is_streamed", true);
    assert_json_field!(link_json, "interfaces", array);

    // Verify priorities object and its nested fields
    // With prio=1-7, priority 1 maps to RealTime and priority 7 maps to Background
    assert_json_field_eq!(link_json, "priorities.start", "RealTime");
    assert_json_field_eq!(link_json, "priorities.end", "Background");

    // Verify reliability field (rel=1 means Reliable)
    assert_json_field_eq!(link_json, "reliability", "Reliable");

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

    // Cleanup subscribers
    transport_subscriber.undeclare().await.unwrap();
    link_subscriber.undeclare().await.unwrap();

    // Cleanup
    router2.close().await.unwrap();
    router1.close().await.unwrap();
}
