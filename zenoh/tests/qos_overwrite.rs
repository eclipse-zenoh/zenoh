//
// Copyright (c) 2025 ZettaScale Technology
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

#![cfg(feature = "unstable")]
#![cfg(target_family = "unix")]
mod common;

use std::time::Duration;

use zenoh::{
    config::WhatAmI,
    qos::{CongestionControl, Priority},
    Config, Session, Wait,
};
use zenoh_config::{WhatAmI as ConfigWhatAmI, ZenohId};

use crate::common::TestSessions;

const SLEEP: Duration = Duration::from_secs(1);

async fn open_routed_clients(
    test_context: &mut TestSessions,
    qos_network_config: &str,
    client2_zid: Option<ZenohId>,
) -> (Session, Session, Session) {
    let mut config_router = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    config_router.set_mode(Some(ConfigWhatAmI::Router)).unwrap();
    config_router
        .insert_json5("qos/network", qos_network_config)
        .unwrap();

    let router = test_context.open_listener_with_cfg(config_router).await;
    tokio::time::sleep(SLEEP).await;

    let mut config_client1 = test_context.get_connector_config();
    config_client1.set_mode(Some(WhatAmI::Client)).unwrap();
    let session1 = test_context.open_connector_with_cfg(config_client1).await;

    let mut config_client2 = test_context.get_connector_config();
    config_client2.set_mode(Some(WhatAmI::Client)).unwrap();
    if let Some(zid) = client2_zid {
        config_client2.set_id(Some(zid)).unwrap();
    }
    let session2 = test_context.open_connector_with_cfg(config_client2).await;

    tokio::time::sleep(SLEEP).await;
    (router, session1, session2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_pub_sub() {
    let value = r#"[
        {
            messages: ["put"],
            key_exprs: ["a/**"],
            overwrite: {
                priority: "real_time",
                congestion_control: "block",
                express: true
            },
            flows: ["ingress"]
        },
        {
            messages: ["delete"],
            key_exprs: ["b/**"],
            overwrite: {
                priority: "interactive_high",
            },
            flows: ["ingress"]
        }
    ]"#;
    test_qos_overwrite_pub_sub_impl(value).await;
}

async fn test_qos_overwrite_pub_sub_impl(qos_network_config: &str) {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let (_router, session1, session2) =
        open_routed_clients(&mut test_context, qos_network_config, None).await;

    let subscriber_a = session2.declare_subscriber("a/**").await.unwrap();
    let subscriber_b = session2.declare_subscriber("b/**").await.unwrap();
    let subscriber_c = session2.declare_subscriber("c/**").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    session1
        .put("a/test", "payload")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    let msg_a = subscriber_a.recv_async().await.unwrap();
    assert_eq!(msg_a.priority(), Priority::RealTime);
    assert_eq!(msg_a.congestion_control(), CongestionControl::Block);
    assert!(msg_a.express());

    session1
        .delete("b/test")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    let msg_b = subscriber_b.recv_async().await.unwrap();
    assert_eq!(msg_b.priority(), Priority::InteractiveHigh);
    assert_eq!(msg_b.congestion_control(), CongestionControl::Drop);
    assert!(!msg_b.express());

    session1
        .put("c/test", "payload")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    let msg_c = subscriber_c.recv_async().await.unwrap();
    assert_eq!(msg_c.priority(), Priority::DataLow);
    assert_eq!(msg_c.congestion_control(), CongestionControl::Drop);
    assert!(!msg_c.express());

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_get_reply() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let (_router, session1, session2) = open_routed_clients(
        &mut test_context,
        r#"[
            {
                messages: ["query"],
                key_exprs: ["a/test"],
                overwrite: {
                    priority: "real_time",
                    congestion_control: "block",
                    express: true
                },
                flows: ["egress"]
            },
        ]"#,
        None,
    )
    .await;

    let queryable = session2.declare_queryable("a/**").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = session1
        .get("a/test")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    let query = queryable.recv_async().await.unwrap();
    assert_eq!(query.priority(), Priority::RealTime);
    assert_eq!(query.congestion_control(), CongestionControl::Block);
    assert!(query.express());

    query.reply("a/test", "reply").express(false).await.unwrap();
    std::mem::drop(query);
    let reply = replies.recv_async().await.unwrap();
    // Reply inherits the QoS of the original query
    assert_eq!(reply.result().unwrap().priority(), Priority::DataLow);
    assert_eq!(
        reply.result().unwrap().congestion_control(),
        CongestionControl::Drop
    );
    assert!(!reply.result().unwrap().express());

    let replies = session1
        .get("a/b/test")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    let query = queryable.recv_async().await.unwrap();
    assert_eq!(query.priority(), Priority::DataLow);
    assert_eq!(query.congestion_control(), CongestionControl::Drop);
    assert!(!query.express());

    query
        .reply("a/b/test", "reply")
        .express(false)
        .await
        .unwrap();
    std::mem::drop(query);

    let reply = replies.recv_async().await.unwrap();
    // Reply inherits the QoS of the original query
    assert_eq!(reply.result().unwrap().priority(), Priority::DataLow);
    assert_eq!(
        reply.result().unwrap().congestion_control(),
        CongestionControl::Drop
    );
    assert!(!reply.result().unwrap().express());

    test_context.close().await;
}

#[test]
#[should_panic(expected = "Invalid Qos Overwrite config: id 'REPEATED' is repeated")]
fn qos_overwrite_config_error_repeated_id() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    config
        .insert_json5(
            "qos/network",
            r#"[
                {
                    id: "REPEATED",
                    messages: ["query"],
                    key_exprs: ["a/**"],
                    overwrite: {
                        priority: "real_time",
                        congestion_control: "block",
                        express: true
                    },
                    flows: ["egress"]
                },
                {
                    id: "REPEATED",
                    messages: ["put"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: ["egress"]
                }
            ]"#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

#[test]
fn qos_overwrite_config_error_wrong_flow() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    assert!(config
        .insert_json5(
            "qos/network",
            r#"
              [
                {
                    messages: ["put"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: ["down"]
                }
              ]
            "#,
        )
        .is_err());
}

#[test]
fn qos_overwrite_config_error_empty_flow() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    assert!(config
        .insert_json5(
            "qos/network",
            r#"
              [
                {
                    messages: ["put"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: []
                }
              ]
            "#,
        )
        .is_err());
}

#[test]
fn qos_overwrite_config_error_empty_message() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    assert!(config
        .insert_json5(
            "qos/network",
            r#"
              [
                {
                    messages: [],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: ["ingress"]
                }
              ]
            "#,
        )
        .is_err());
}

#[test]
fn qos_overwrite_config_error_empty_interface() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    assert!(config
        .insert_json5(
            "qos/network",
            r#"
              [
                {
                    interfaces: [],
                    messages: ["put"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: ["ingress"]
                }
              ]
            "#,
        )
        .is_err());
}

#[test]
fn qos_overwrite_config_ok_no_flow() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    config
        .insert_json5(
            "qos/network",
            r#"
              [
                {
                    messages: ["put"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    }
                }
              ]
            "#,
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_link_protocols() {
    let value = r#"[
        {
            link_protocols: [ "tcp" ],
            messages: ["put"],
            key_exprs: ["a/**"],
            overwrite: {
                priority: "real_time",
                congestion_control: "block",
                express: true
            },
            flows: ["ingress"]
        },
        {
            link_protocols: [ "udp" ],
            messages: ["put"],
            key_exprs: ["c/**"],
            overwrite: {
                priority: "real_time",
                congestion_control: "block",
                express: true
            },
            flows: ["ingress"]
        },
        {
            messages: ["delete"],
            key_exprs: ["b/**"],
            overwrite: {
                priority: "interactive_high",
            },
            flows: ["ingress"]
        }
    ]"#;
    test_qos_overwrite_pub_sub_impl(value).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_zids() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let client2_zid = ZenohId::default();
    let qos_network = format!(
        r#"[
            {{
                zids: ["{}"],
                messages: ["put"],
                key_exprs: ["**"],
                overwrite: {{
                    priority: "real_time",
                }},
                flows: ["egress"]
            }}
        ]"#,
        client2_zid
    );
    let (_router, session1, session2) =
        open_routed_clients(&mut test_context, &qos_network, Some(client2_zid)).await;

    let subscriber = session2.declare_subscriber("a/**").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    session1.put("a/test", "payload").await.unwrap();
    let msg = subscriber.recv_async().await.unwrap();
    assert_eq!(msg.priority(), Priority::RealTime);

    test_context.close().await;
}
