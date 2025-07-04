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

#![cfg(all(feature = "unstable", feature = "internal_config"))]
#![cfg(target_family = "unix")]
use std::time::Duration;

use zenoh::{
    config::WhatAmI,
    qos::{CongestionControl, Priority},
    Config, Wait,
};
use zenoh_config::ZenohId;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

async fn get_basic_router_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.set_id(Some(ZenohId::default())).unwrap();
    config
}

async fn get_basic_client_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.set_id(Some(ZenohId::default())).unwrap();
    config
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
    test_qos_overwrite_pub_sub_impl(27601, value).await;
}

async fn test_qos_overwrite_pub_sub_impl(port: u16, qos_network_config: &str) {
    zenoh::init_log_from_env_or("error");
    let mut config_router = get_basic_router_config(port).await;
    let config_client1 = get_basic_client_config(port).await;
    let config_client2 = get_basic_client_config(port).await;
    config_router
        .insert_json5("qos/network", qos_network_config)
        .unwrap();

    let _router = ztimeout!(zenoh::open(config_router)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(zenoh::open(config_client1)).unwrap();
    let session2 = ztimeout!(zenoh::open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;

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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_get_reply() {
    zenoh::init_log_from_env_or("error");
    let mut config_router = get_basic_router_config(27602).await;
    let config_client1 = get_basic_client_config(27602).await;
    let config_client2 = get_basic_client_config(27602).await;
    config_router
        .insert_json5(
            "qos/network",
            r#"[
                {
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
                    messages: ["reply"],
                    key_exprs: ["a/b/**"],
                    overwrite: {
                        priority: "interactive_high",
                    },
                    flows: ["egress"]
                }
            ]"#,
        )
        .unwrap();

    let _router = ztimeout!(zenoh::open(config_router)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(zenoh::open(config_client1)).unwrap();
    let session2 = ztimeout!(zenoh::open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;

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
    //assert_eq!(query.priority(), Priority::RealTime);
    //assert_eq!(query.congestion_control(), CongestionControl::Block);
    //assert!(query.express());

    query
        .reply("a/test", "reply")
        .priority(Priority::DataHigh)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    std::mem::drop(query);
    let reply = replies.recv_async().await.unwrap();
    assert_eq!(reply.result().unwrap().priority(), Priority::DataHigh);
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
    //assert_eq!(query.priority(), Priority::RealTime);
    //assert_eq!(query.congestion_control(), CongestionControl::Block);
    //assert!(query.express());

    query
        .reply("a/b/test", "reply")
        .priority(Priority::DataHigh)
        .congestion_control(CongestionControl::Drop)
        .express(false)
        .await
        .unwrap();
    std::mem::drop(query);

    let reply = replies.recv_async().await.unwrap();
    assert_eq!(
        reply.result().unwrap().priority(),
        Priority::InteractiveHigh
    );
    assert_eq!(
        reply.result().unwrap().congestion_control(),
        CongestionControl::Drop
    );
    assert!(!reply.result().unwrap().express());
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
                    messages: ["reply"],
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
                    messages: ["reply"],
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
                    messages: ["reply"],
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
                    messages: ["reply"],
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
                    messages: ["reply"],
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
    test_qos_overwrite_pub_sub_impl(27603, value).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_qos_overwrite_zids() {
    zenoh::init_log_from_env_or("error");
    let port = 27604;
    let mut config_router = get_basic_router_config(port).await;
    let config_client1 = get_basic_client_config(port).await;
    let config_client2 = get_basic_client_config(port).await;
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
        config_client2.id().unwrap()
    );
    config_router
        .insert_json5("qos/network", &qos_network)
        .unwrap();

    let _router = ztimeout!(zenoh::open(config_router)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(zenoh::open(config_client1)).unwrap();
    let session2 = ztimeout!(zenoh::open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let subscriber = session2.declare_subscriber("a/**").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    session1.put("a/test", "payload").await.unwrap();
    let msg = subscriber.recv_async().await.unwrap();
    assert_eq!(msg.priority(), Priority::RealTime);
}
