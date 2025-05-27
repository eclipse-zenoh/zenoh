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

mod common;

use std::{
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use tokio::runtime::Handle;
use zenoh::{sample::SampleKind, Session};
use zenoh_core::{zlock, ztimeout, Wait};

use crate::common::{open_client, open_router, TestOpenBuilder};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const KEY_EXPR: &str = "test/demo";
const VALUE: &str = "zenoh";

#[test]
#[should_panic(expected = "config lists must be provided")]
fn test_acl_config_missing_list() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "deny"
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "Rule id must be unique")]
fn test_acl_config_repeated_rule_id() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "Subject id must be unique")]
fn test_acl_config_repeated_subject_id() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "Policy id must be unique")]
fn test_acl_config_repeated_policy_id() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "does not exist in rules list")]
fn test_acl_config_non_existent_rule_in_policy() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "does not exist in subjects list")]
fn test_acl_config_non_existent_subject_in_policy() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
}

#[test]
#[should_panic(expected = "empty")]
fn test_acl_config_empty_link_protocols_list() {
    zenoh::init_log_from_env_or("error");

    // empty link_protocols list
    let acl_cfg = r#"{
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
    }"#;
    open_router().with_json5("access_control", acl_cfg).wait();
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

fn open_router_session(acl_config: &str) -> TestOpenBuilder {
    println!("Opening router session");
    open_router().with_json5("access_control", acl_config)
}

async fn close_router_session(s: Session) {
    println!("Closing router session");
    ztimeout!(s.close()).unwrap();
}

async fn open_client_session(router: &Session) -> (Session, Session) {
    println!("Opening client sessions");
    tokio::join!(
        open_client().connect_to(router),
        open_client().connect_to(router)
    )
}

async fn close_sessions(s01: Session, s02: Session) {
    println!("Closing client sessions");
    ztimeout!(s01.close()).unwrap();
    ztimeout!(s02.close()).unwrap();
}

async fn test_pub_sub_deny() {
    println!("test_pub_sub_deny");

    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "deny",
        "rules": [],
        "subjects": [],
        "policies": [],
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_session(&router));
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
    close_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_allow() {
    println!("test_pub_sub_allow");

    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "allow",
        "rules": [],
        "subjects": [],
        "policies": [],
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_session(&router));
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

    close_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_allow_then_deny() {
    println!("test_pub_sub_allow_then_deny");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_session(&router));
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
    close_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_deny_then_allow() {
    println!("test_pub_sub_deny_then_allow");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_session(&router));
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
    close_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_deny() {
    println!("test_get_qbl_deny");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_allow() {
    println!("test_get_qbl_allow");

    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "allow",
        "rules": [],
        "subjects": [],
        "policies": [],
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_eq!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_deny_then_allow() {
    println!("test_get_qbl_deny_then_allow");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_eq!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_allow_then_deny() {
    println!("test_get_qbl_allow_then_deny");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_reply_deny() {
    println!("test_reply_deny");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_reply_allow_then_deny() {
    println!("test_reply_allow_then_deny");

    let acl_cfg = r#"{
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
}"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (get_session, qbl_session) = ztimeout!(open_client_session(&router));
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
                Err(e) => println!("Error : {:?}", e),
            }
        }
        tokio::time::sleep(SLEEP).await;
        assert_ne!(received_value, VALUE);
        ztimeout!(qbl.undeclare()).unwrap();
    }
    close_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_deny() {
    println!("test_liveliness_deny");

    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "deny",
        "rules": [],
        "subjects": [],
        "policies": [],
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_allow() {
    println!("test_liveliness_allow");

    let acl_cfg = r#"{
        "enabled": true,
        "default_permission": "allow",
        "rules": [],
        "subjects": [],
        "policies": [],
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_allow_deny_token() {
    println!("test_liveliness_allow_deny_token");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_deny_allow_token() {
    println!("test_liveliness_deny_allow_token");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_allow_deny_sub() {
    println!("test_liveliness_allow_deny_sub");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_deny_allow_sub() {
    println!("test_liveliness_deny_allow_sub");

    let acl_cfg = r#"{
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
        }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_allow_deny_query() {
    println!("test_liveliness_allow_deny_query");

    let acl_cfg = r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            id: "filter query",
                            permission: "deny",
                            messages: ["liveliness_query"],
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
                }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

async fn test_liveliness_deny_allow_query() {
    println!("test_liveliness_deny_allow_query");

    let acl_cfg = r#"{
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
        }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (reader_session, writer_session) = ztimeout!(open_client_session(&router));

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
            Err(e) => println!("Error : {:?}", e),
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
    close_sessions(reader_session, writer_session).await;
    close_router_session(router).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_query_ingress_deny() {
    zenoh::init_log_from_env_or("error");

    let acl_cfg = r#"{
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
      }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (session1, session2) = ztimeout!(open_client_session(&router));
    tokio::time::sleep(SLEEP).await;

    let _qbl = session1.declare_queryable("test/ingress").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = session2.get("test/ingress").await.unwrap();

    assert!(replies.recv_async().await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_query_egress_deny() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
      }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    let (session1, session2) = ztimeout!(open_client_session(&router));
    tokio::time::sleep(SLEEP).await;

    let _qbl = session1.declare_queryable("test/egress").await.unwrap();
    tokio::time::sleep(SLEEP).await;
    let replies = session2.get("test/egress").await.unwrap();

    assert!(replies.recv_async().await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_liveliness_query_ingress_deny() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
      }"#;
    let router = ztimeout!(open_router_session(acl_cfg));
    tokio::time::sleep(SLEEP).await;
    let (session1, session2) = ztimeout!(open_client_session(&router));
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_acl_liveliness_query_egress_deny() {
    zenoh::init_log_from_env_or("error");
    let acl_cfg = r#"{
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
      }"#;
    let router = ztimeout!(open_router());
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(open_client()
        .connect_to(&router)
        .with_json5("access_control", acl_cfg));
    let session2 = ztimeout!(open_client().connect_to(&router));
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
}
