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

#![cfg(feature = "unstable_config")]
#![cfg(target_family = "unix")]
mod test {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use tokio::runtime::Handle;
    use zenoh::{config::WhatAmI, sample::SampleKind, Config, Session};
    use zenoh_config::{EndPoint, ModeDependentValue};
    use zenoh_core::{zlock, ztimeout};

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_pub_sub() {
        zenoh::init_log_from_env_or("error");
        test_pub_sub_deny(27447).await;
        test_pub_sub_allow(27447).await;
        test_pub_sub_deny_then_allow(27447).await;
        test_pub_sub_allow_then_deny(27447).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_get_queryable() {
        zenoh::init_log_from_env_or("error");
        test_get_qbl_deny(27448).await;
        test_get_qbl_allow(27448).await;
        test_get_qbl_allow_then_deny(27448).await;
        test_get_qbl_deny_then_allow(27448).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_queryable_reply() {
        zenoh::init_log_from_env_or("error");
        // Only test cases not covered by `test_acl_get_queryable`
        test_reply_deny(27449).await;
        test_reply_allow_then_deny(27449).await;
    }

    async fn get_basic_router_config(port: u16) -> Config {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
    }

    async fn close_router_session(s: Session) {
        println!("Closing router session");
        ztimeout!(s.close()).unwrap();
    }

    async fn get_client_sessions(port: u16) -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tcp/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();

        let s01 = ztimeout!(zenoh::open(config)).unwrap();

        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tcp/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn close_sessions(s01: Session, s02: Session) {
        println!("Closing client sessions");
        ztimeout!(s01.close()).unwrap();
        ztimeout!(s02.close()).unwrap();
    }

    async fn test_pub_sub_deny(port: u16) {
        println!("test_pub_sub_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions(port).await;
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
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow(port: u16) {
        println!("test_pub_sub_allow");
        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions(port).await;
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
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow_then_deny(port: u16) {
        println!("test_pub_sub_allow_then_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions(port).await;
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
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_pub_sub_deny_then_allow(port: u16) {
        println!("test_pub_sub_deny_then_allow");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions(port).await;
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
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny(port: u16) {
        println!("test_get_qbl_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow(port: u16) {
        println!("test_get_qbl_allow");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny_then_allow(port: u16) {
        println!("test_get_qbl_deny_then_allow");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow_then_deny(port: u16) {
        println!("test_get_qbl_allow_then_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_reply_deny(port: u16) {
        println!("test_reply_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }

    async fn test_reply_allow_then_deny(port: u16) {
        println!("test_reply_allow_then_deny");

        let mut config_router = get_basic_router_config(port).await;
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

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().deserialize::<String>().unwrap();
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
        close_router_session(session).await;
    }
}
