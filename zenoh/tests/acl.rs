//
// Copyright (c) 2024 ZettaScale Technology
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


use std::{
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use zenoh_test::TestSessions;
use tokio::runtime::Handle;
use zenoh::{config::WhatAmI, sample::SampleKind};
use zenoh_config::Config;
use zenoh_core::{zlock, ztimeout};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const KEY_EXPR: &str = "test/demo";
const KEY_EXPR2: &str = "test/demo2";
const VALUE: &str = "zenoh";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_config() {
    zenoh::init_log_from_env_or("error");
    test_acl_config_format().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_pub_sub() {
    zenoh::init_log_from_env_or("error");
    test_pub_sub_deny().await;
    test_pub_sub_allow().await;
    test_pub_sub_deny_then_allow().await;
    test_pub_sub_allow_then_deny().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_get_queryable() {
    zenoh::init_log_from_env_or("error");
    test_get_qbl_deny().await;
    test_get_qbl_allow().await;
    test_get_qbl_allow_then_deny().await;
    test_get_qbl_deny_then_allow().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_queryable_reply() {
    zenoh::init_log_from_env_or("error");
    // Only test cases not covered by `test_acl_get_queryable`
    test_reply_deny().await;
    test_reply_allow_then_deny().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_liveliness() {
    zenoh::init_log_from_env_or("error");

    test_liveliness_allow().await;
    test_liveliness_deny().await;

    test_liveliness_allow_deny_token().await;
    test_liveliness_deny_allow_token().await;

    test_liveliness_allow_deny_sub().await;
    test_liveliness_deny_allow_sub().await;

    test_liveliness_allow_deny_query().await;
    test_liveliness_deny_allow_query().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_interface_names() {
    zenoh::init_log_from_env_or("error");

    test_pub_sub_network_interface().await;
}

async fn get_basic_router_config() -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:0".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config
}

fn get_basic_client_config(test_context: &TestSessions) -> Config {
    let mut config = test_context.get_connector_config();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
}

fn get_loopback_client_config(test_context: &TestSessions) -> Config {
    let loopback_locators = test_context
        .locators()
        .into_iter()
        .filter_map(|locator| {
            let port = locator.address().as_str().rsplit(':').next()?;
            format!("tcp/127.0.0.1:{port}").parse().ok()
        })
        .collect();
    let mut config = test_context.get_connector_config_with_endpoint(loopback_locators);
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
}

async fn get_client_sessions(test_context: &mut TestSessions) -> (zenoh::Session, zenoh::Session) {
    println!("Opening client sessions");
    let s01 = test_context
        .open_connector_with_cfg(get_basic_client_config(test_context))
        .await;
    let s02 = test_context
        .open_connector_with_cfg(get_basic_client_config(test_context))
        .await;
    (s01, s02)
}

async fn test_acl_config_format() {
    println!("test_acl_config_format");
    let mut config_router = get_basic_router_config().await;

    // missing lists
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny"
                }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("config lists must be provided")));

    // repeated rule id
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["bar"],
                    },
                ],
                "subjects": [{id: "all"}],
                "policies": [
                    {
                        rules: ["r1"],
                        subjects: ["all"],
                    }
                ],
            }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("Rule id must be unique")));

    // repeated subject id
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                ],
                "subjects": [
                    {
                        id: "s1",
                        interfaces: ["lo"],
                    },
                    {
                        id: "s1",
                        interfaces: ["lo0"],
                    },
                ],
                "policies": [
                    {
                        rules: ["r1"],
                        subjects: ["s1"],
                    }
                ],
            }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("Subject id must be unique")));

    // repeated policy id
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                    {
                        "id": "r2",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["bar"],
                    },
                ],
                "subjects": [{id: "all"}],
                "policies": [
                    {
                        id: "p1",
                        rules: ["r1"],
                        subjects: ["all"],
                    },
                    {
                        id: "p1",
                        rules: ["r2"],
                        subjects: ["all"],
                    }
                ],
            }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("Policy id must be unique")));

    // non-existent rule in policy
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                    {
                        "id": "r2",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["bar"],
                    },
                ],
                "subjects": [{id: "all"}],
                "policies": [
                    {
                        id: "p1",
                        rules: ["r1"],
                        subjects: ["all"],
                    },
                    {
                        id: "p2",
                        rules: ["NON-EXISTENT"],
                        subjects: ["all"],
                    }
                ],
            }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("does not exist in rules list")));

    // non-existent subject in policy
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                    {
                        "id": "r2",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["bar"],
                    },
                ],
                "subjects": [{id: "all"}],
                "policies": [
                    {
                        id: "p1",
                        rules: ["r1"],
                        subjects: ["all"],
                    },
                    {
                        id: "p2",
                        rules: ["r2"],
                        subjects: ["NON-EXISTENT"],
                    }
                ],
            }"#,
        )
        .unwrap();
    assert!(ztimeout!(zenoh::open(config_router.clone()))
        .is_err_and(|e| e.to_string().contains("does not exist in subjects list")));

    // empty link_protocols list
    assert!(config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "allow",
                        "flows": ["egress", "ingress"],
                        "messages": ["put"],
                        "key_exprs": ["foo"],
                    },
                ],
                "subjects": [
                    {
                        id: "s1",
                        link_protocols: [], // will Err - should not be empty
                    },
                ],
                "policies": [
                    {
                        id: "p1",
                        rules: ["r1"],
                        subjects: ["s1"],
                    },
                ],
            }"#,
        )
        .is_err());
}

async fn test_pub_sub_deny() {
    println!("test_pub_sub_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [],
                    "subjects": [],
                    "policies": [],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    println!("Opening client sessions");
    let sub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    let pub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    {
        let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let deleted = Arc::new(Mutex::new(false));

        let temp_recv_value = received_value.clone();
        let deleted_clone = deleted.clone();
        let subscriber = sub_session
            .declare_subscriber(KEY_EXPR)
            .callback(move |sample| {
                if sample.kind() == SampleKind::Put {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                } else if sample.kind() == SampleKind::Delete {
                    let mut deleted = zlock!(deleted_clone);
                    *deleted = true;
                }
            })
            .await
            .unwrap();

        tokio::time::sleep(SLEEP).await;
        publisher.put(VALUE).await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert_ne!(*zlock!(received_value), VALUE);

        publisher.delete().await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(!(*zlock!(deleted)));
        ztimeout!(subscriber.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_pub_sub_allow() {
    println!("test_pub_sub_allow");
    let mut test_context = TestSessions::new();
    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [],
                    "subjects": [],
                    "policies": [],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    println!("Opening client sessions");
    let sub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    let pub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    {
        let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let deleted = Arc::new(Mutex::new(false));

        let temp_recv_value = received_value.clone();
        let deleted_clone = deleted.clone();
        let subscriber = sub_session
            .declare_subscriber(KEY_EXPR)
            .callback(move |sample| {
                if sample.kind() == SampleKind::Put {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                } else if sample.kind() == SampleKind::Delete {
                    let mut deleted = zlock!(deleted_clone);
                    *deleted = true;
                }
            })
            .await
            .unwrap();

        tokio::time::sleep(SLEEP).await;
        publisher.put(VALUE).await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert_eq!(*zlock!(received_value), VALUE);

        publisher.delete().await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(*zlock!(deleted));
        ztimeout!(subscriber.undeclare()).unwrap();
    }

    test_context.close().await;
}

async fn test_pub_sub_allow_then_deny() {
    println!("test_pub_sub_allow_then_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "put",
                                "delete",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "interfaces": [
                                "lo", "lo0"
                            ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    println!("Opening client sessions");
    let sub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    let pub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    {
        let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let deleted = Arc::new(Mutex::new(false));

        let temp_recv_value = received_value.clone();
        let deleted_clone = deleted.clone();
        let subscriber = sub_session
            .declare_subscriber(KEY_EXPR)
            .callback(move |sample| {
                if sample.kind() == SampleKind::Put {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                } else if sample.kind() == SampleKind::Delete {
                    let mut deleted = zlock!(deleted_clone);
                    *deleted = true;
                }
            })
            .await
            .unwrap();

        tokio::time::sleep(SLEEP).await;
        publisher.put(VALUE).await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert_ne!(*zlock!(received_value), VALUE);

        publisher.delete().await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(!(*zlock!(deleted)));
        ztimeout!(subscriber.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_pub_sub_deny_then_allow() {
    println!("test_pub_sub_deny_then_allow");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "put",
                                "delete",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "interfaces": [
                                "lo", "lo0"
                            ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (sub_session, pub_session) = get_client_sessions(&mut test_context).await;
    {
        let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let deleted = Arc::new(Mutex::new(false));

        let temp_recv_value = received_value.clone();
        let deleted_clone = deleted.clone();
        let subscriber = sub_session
            .declare_subscriber(KEY_EXPR)
            .callback(move |sample| {
                if sample.kind() == SampleKind::Put {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                } else if sample.kind() == SampleKind::Delete {
                    let mut deleted = zlock!(deleted_clone);
                    *deleted = true;
                }
            })
            .await
            .unwrap();

        tokio::time::sleep(SLEEP).await;
        publisher.put(VALUE).await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert_eq!(*zlock!(received_value), VALUE);

        publisher.delete().await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(*zlock!(deleted));
        ztimeout!(subscriber.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_get_qbl_deny() {
    println!("test_get_qbl_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "allow reply",
                            "permission": "allow",
                            "messages": ["reply"],
                            "flows": ["egress", "ingress"],
                            "key_exprs": ["test/demo"],
                        }
                    ],
                    "subjects": [
                        { "id": "all" }
                    ],
                    "policies": [
                        {
                            "rules": ["allow reply"],
                            "subjects": ["all"],
                        }
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_get_qbl_allow() {
    println!("test_get_qbl_allow");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [],
                    "subjects": [],
                    "policies": [],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_eq!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_get_qbl_deny_then_allow() {
    println!("test_get_qbl_deny_then_allow");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "interfaces": [
                                "lo", "lo0"
                            ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
        )
        .unwrap();

    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_eq!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_get_qbl_allow_then_deny() {
    println!("test_get_qbl_allow_then_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "query",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "interfaces": [
                                "lo", "lo0"
                            ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_reply_deny() {
    println!("test_reply_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "allow get/declare qbl",
                            "permission": "allow",
                            "messages": ["query", "declare_queryable"],
                            "key_exprs": ["test/demo"],
                        }
                    ],
                    "subjects": [
                        { "id": "all" }
                    ],
                    "policies": [
                        {
                            "rules": ["allow get/declare qbl"],
                            "subjects": ["all"],
                        }
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_reply_allow_then_deny() {
    println!("test_reply_allow_then_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                "enabled": true,
                "default_permission": "allow",
                "rules": [
                    {
                        "id": "r1",
                        "permission": "deny",
                        "messages": ["reply"],
                        "flows": ["egress", "ingress"],
                        "key_exprs": ["test/demo"],
                    },
                ],
                "subjects": [
                    {
                        "id": "s1",
                        "interfaces": [
                            "lo", "lo0"
                        ],
                    }
                ],
                "policies": [
                    {
                        "rules": ["r1"],
                        "subjects": ["s1"],
                    }
                ]
            }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (get_session, qbl_session) = get_client_sessions(&mut test_context).await;
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
                });
            }))
        .unwrap();

        tokio::time::sleep(SLEEP).await;
        let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
        while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
            match reply.result() {
                Ok(sample) => {
                    received_value = sample.payload().try_to_string().unwrap().into_owned();
                    break;
                }
                Err(e) => println!("Error : {e:?}"),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    test_context.close().await;
}

async fn test_liveliness_deny() {
    println!("test_liveliness_deny");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [],
                    "subjects": [],
                    "policies": [],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_allow() {
    println!("test_liveliness_allow");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [],
                    "subjects": [],
                    "policies": [],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_allow_deny_token() {
    println!("test_liveliness_allow_deny_token");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            id: "filter token",
                            permission: "deny",
                            messages: ["liveliness_token"],
                            flows: ["ingress", "egress"],
                            key_exprs: ["test/demo"],
                        },
                    ],
                    "subjects": [
                        { id: "all" },
                    ],
                    "policies": [
                        {
                            "subjects": ["all"],
                            "rules": ["filter token"],
                        },
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_deny_allow_token() {
    println!("test_liveliness_deny_allow_token");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            id: "filter token",
                            permission: "allow",
                            messages: ["liveliness_token"],
                            flows: ["ingress", "egress"],
                            key_exprs: ["test/demo"],
                        },
                    ],
                    "subjects": [
                        { id: "all" },
                    ],
                    "policies": [
                        {
                            "subjects": ["all"],
                            "rules": ["filter token"],
                        },
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_allow_deny_sub() {
    println!("test_liveliness_allow_deny_sub");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            id: "filter sub",
                            permission: "deny",
                            messages: ["declare_liveliness_subscriber"],
                            flows: ["ingress", "egress"],
                            key_exprs: ["test/demo"],
                        },
                    ],
                    "subjects": [
                        { id: "all" },
                    ],
                    "policies": [
                        {
                            "subjects": ["all"],
                            "rules": ["filter sub"],
                        },
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_deny_allow_sub() {
    println!("test_liveliness_deny_allow_sub");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                        "enabled": true,
                        "default_permission": "deny",
                        "rules": [
                            {
                                id: "filter sub",
                                permission: "allow",
                                messages: ["declare_liveliness_subscriber", "liveliness_token"],
                                flows: ["ingress", "egress"],
                                key_exprs: ["test/demo"],
                            },
                        ],
                        "subjects": [
                            { id: "all" },
                        ],
                        "policies": [
                            {
                                "subjects": ["all"],
                                "rules": ["filter sub"],
                            },
                        ],
                    }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token.load(std::sync::atomic::Ordering::Relaxed));

    // NOTE(fuzzypixelz): had this query been on KEY_EXPR, it would've returned the token above.
    // This is because the client gateway registers tokens received for the future interest.

    // test if query receives token reply
    let _liveliness2 = writer_session
        .liveliness()
        .declare_token(KEY_EXPR2)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    reader_session
        .liveliness()
        .get(KEY_EXPR2)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_allow_deny_query() {
    println!("test_liveliness_allow_deny_query");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            id: "filter query",
                            permission: "deny",
                            messages: ["liveliness_query"],
                            flows: ["ingress", "egress"],
                            key_exprs: ["test/demo2"],
                        },
                    ],
                    "subjects": [
                        { id: "all" },
                    ],
                    "policies": [
                        {
                            "subjects": ["all"],
                            "rules": ["filter query"],
                        },
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token.load(std::sync::atomic::Ordering::Relaxed));

    // NOTE(fuzzypixelz): had this query been on KEY_EXPR, it would've returned the token above.
    // This is because the client gateway registers tokens received for the future interest.

    // test if query receives token reply
    let _liveliness2 = writer_session
        .liveliness()
        .declare_token(KEY_EXPR2)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    reader_session
        .liveliness()
        .get(KEY_EXPR2)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

async fn test_liveliness_deny_allow_query() {
    println!("test_liveliness_deny_allow_query");
    let mut test_context = TestSessions::new();

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
                        "enabled": true,
                        "default_permission": "deny",
                        "rules": [
                            {
                                id: "filter query",
                                permission: "allow",
                                messages: ["liveliness_query", "liveliness_token"],
                                flows: ["ingress", "egress"],
                                key_exprs: ["test/demo"],
                            },
                        ],
                        "subjects": [
                            { id: "all" },
                        ],
                        "policies": [
                            {
                                "subjects": ["all"],
                                "rules": ["filter query"],
                            },
                        ],
                    }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    let (reader_session, writer_session) = get_client_sessions(&mut test_context).await;

    let received_token = Arc::new(AtomicBool::new(false));
    let dropped_token = Arc::new(AtomicBool::new(false));
    let received_token_reply = Arc::new(AtomicBool::new(false));

    let cloned_received_token = received_token.clone();
    let cloned_dropped_token = dropped_token.clone();
    let cloned_received_token_reply = received_token_reply.clone();

    let subscriber = reader_session
        .liveliness()
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            if sample.kind() == SampleKind::Put {
                cloned_received_token.store(true, std::sync::atomic::Ordering::Relaxed);
            } else if sample.kind() == SampleKind::Delete {
                cloned_dropped_token.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    // test if sub receives token declaration
    let liveliness = writer_session
        .liveliness()
        .declare_token(KEY_EXPR)
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!received_token.load(std::sync::atomic::Ordering::Relaxed));

    // test if query receives token reply
    reader_session
        .liveliness()
        .get(KEY_EXPR)
        .timeout(TIMEOUT)
        .callback(move |reply| match reply.result() {
            Ok(_) => {
                cloned_received_token_reply.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => println!("Error : {e:?}"),
        })
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(received_token_reply.load(std::sync::atomic::Ordering::Relaxed));

    // test if sub receives token undeclaration
    ztimeout!(liveliness.undeclare()).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(!dropped_token.load(std::sync::atomic::Ordering::Relaxed));

    ztimeout!(subscriber.undeclare()).unwrap();
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_query_ingress_deny() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules": [
                {
                    "id": "deny query ingress",
                    "permission": "deny",
                    "messages": ["query"],
                    "flows": ["ingress"],
                    "key_exprs": ["**"],
                }
            ],
            "subjects": [
                { "id": "all" }
            ],
            "policies": [
                {
                    "rules": ["deny query ingress"],
                    "subjects": ["all"],
                }
            ],
          }"#,
        )
        .unwrap();

    let _router = test_context.open_listener_with_cfg(config_router).await;
    let (session1, session2) = get_client_sessions(&mut test_context).await;
    tokio::time::sleep(SLEEP).await;

    let _qbl = session1.declare_queryable("test/ingress").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = session2.get("test/ingress").await.unwrap();

    assert!(replies.recv_async().await.is_err());
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_query_egress_deny() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules": [
                {
                    "id": "deny query egress",
                    "permission": "deny",
                    "messages": ["query"],
                    "flows": ["egress"],
                    "key_exprs": ["**"],
                }
            ],
            "subjects": [
                { "id": "all" }
            ],
            "policies": [
                {
                    "rules": ["deny query egress"],
                    "subjects": ["all"],
                }
            ],
          }"#,
        )
        .unwrap();

    let _router = test_context.open_listener_with_cfg(config_router).await;
    let (session1, session2) = get_client_sessions(&mut test_context).await;
    tokio::time::sleep(SLEEP).await;

    let _qbl = session1.declare_queryable("test/egress").await.unwrap();
    tokio::time::sleep(SLEEP).await;
    let replies = session2.get("test/egress").await.unwrap();

    assert!(replies.recv_async().await.is_err());
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_liveliness_query_ingress_deny() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "access_control",
            r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules": [
                {
                    "id": "deny query ingress",
                    "permission": "deny",
                    "messages": ["liveliness_query"],
                    "flows": ["ingress"],
                    "key_exprs": ["**"],
                }
            ],
            "subjects": [
                { "id": "all" }
            ],
            "policies": [
                {
                    "rules": ["deny query ingress"],
                    "subjects": ["all"],
                }
            ],
          }"#,
        )
        .unwrap();

    let _router = test_context.open_listener_with_cfg(config_router).await;
    tokio::time::sleep(SLEEP).await;
    let (session1, session2) = get_client_sessions(&mut test_context).await;
    tokio::time::sleep(SLEEP).await;

    let _token = session1
        .liveliness()
        .declare_token("test/token/ingress")
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = session2
        .liveliness()
        .get("test/token/ingress")
        .timeout(Duration::from_secs(10))
        .await
        .unwrap();

    assert!(replies.recv_timeout(Duration::from_secs(1)).is_err());
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_liveliness_query_egress_deny() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let config_router = get_basic_router_config().await;
    let _router = test_context.open_listener_with_cfg(config_router).await;
    tokio::time::sleep(SLEEP).await;
    let mut config_client1 = get_basic_client_config(&test_context);
    config_client1
        .insert_json5(
            "access_control",
            r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules": [
                {
                    "id": "deny query egress",
                    "permission": "deny",
                    "messages": ["liveliness_query"],
                    "flows": ["egress"],
                    "key_exprs": ["**"],
                }
            ],
            "subjects": [
                { "id": "all" }
            ],
            "policies": [
                {
                    "rules": ["deny query egress"],
                    "subjects": ["all"],
                }
            ],
          }"#,
        )
        .unwrap();
    let session1 = test_context.open_connector_with_cfg(config_client1).await;
    let session2 = test_context
        .open_connector_with_cfg(get_basic_client_config(&test_context))
        .await;
    tokio::time::sleep(SLEEP).await;

    let _token = session2
        .liveliness()
        .declare_token("test/token/egress")
        .await
        .unwrap();
    tokio::time::sleep(SLEEP).await;
    let replies = session1
        .liveliness()
        .get("test/token/egress")
        .timeout(Duration::from_secs(10))
        .await
        .unwrap();

    assert!(replies.recv_timeout(Duration::from_secs(1)).is_err());
    test_context.close().await;
}

async fn test_pub_sub_network_interface() {
    println!("test_pub_sub_network_interface");
    let mut test_context = TestSessions::new();

    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .endpoints
        .set(vec!["tcp/[::]:0".parse().unwrap()])
        .unwrap();
    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    config_router
        .insert_json5(
            "access_control",
            r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            id: "allow pub/sub",
                            permission: "allow",
                            messages: ["put", "delete", "declare_subscriber"],
                            flows: ["ingress", "egress"],
                            key_exprs: ["**"],
                        },
                    ],
                    "subjects": [
                        {
                            id: "loopback",
                            interfaces: ["lo", "lo0"],
                        }
                    ],
                    "policies": [
                        {
                            rules: ["allow pub/sub"],
                            subjects: ["loopback"],
                        }
                    ],
                }"#,
        )
        .unwrap();
    println!("Opening router session");

    let _session = test_context.open_listener_with_cfg(config_router).await;
    println!("Opening client sessions");
    let sub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    let pub_session = test_context
        .open_connector_with_cfg(get_loopback_client_config(&test_context))
        .await;
    {
        let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let deleted = Arc::new(Mutex::new(false));

        let temp_recv_value = received_value.clone();
        let deleted_clone = deleted.clone();
        let subscriber = sub_session
            .declare_subscriber(KEY_EXPR)
            .callback(move |sample| {
                if sample.kind() == SampleKind::Put {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                } else if sample.kind() == SampleKind::Delete {
                    let mut deleted = zlock!(deleted_clone);
                    *deleted = true;
                }
            })
            .await
            .unwrap();

        tokio::time::sleep(SLEEP).await;
        publisher.put(VALUE).await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert_eq!(*zlock!(received_value), VALUE);

        publisher.delete().await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(*zlock!(deleted));
        ztimeout!(subscriber.undeclare()).unwrap();
    }
    test_context.close().await;
}
