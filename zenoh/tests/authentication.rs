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
mod test {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::runtime::Handle;
    use zenoh::prelude::r#async::*;
    use zenoh_core::{zlock, ztimeout};

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    #[ignore]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication() {
        zenoh_util::try_init_log_from_env();
        test_pub_sub_deny_then_allow_tls().await;
        test_pub_sub_allow_then_deny_tls().await;
        test_get_qbl_allow_then_deny_tls().await;
        test_get_qbl_deny_then_allow_tls().await;
        test_pub_sub_deny_then_allow_quic().await;
        test_pub_sub_allow_then_deny_quic().await;
        test_get_qbl_allow_then_deny_quic().await;
        test_get_qbl_deny_then_allow_quic().await;
        test_pub_sub_deny_then_allow_usrpswd().await;
        test_pub_sub_allow_then_deny_usrpswd().await;
        test_get_qbl_allow_then_deny_usrpswd().await;
        test_get_qbl_deny_then_allow_usrpswd().await;
    }
    async fn get_basic_router_config_tls() -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec!["tls/127.0.0.1:7447".parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
        "link": {
          "protocols": [
            "tls"
          ],
          "tls": {
            "server_private_key": "tests/testfiles/serversidekey.pem",
            "server_certificate": "tests/testfiles/serverside.pem",
            "root_ca_certificate": "tests/testfiles/ca.pem",
            "client_auth": true,
            "server_name_verification": false
          },
        },
      }"#,
            )
            .unwrap();
        config
    }
    async fn get_basic_router_config_quic() -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec!["quic/127.0.0.1:7447".parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
        "link": {
          "protocols": [
            "quic"
          ],
          "tls": {
            "server_private_key": "tests/testfiles/serversidekey.pem",
            "server_certificate": "tests/testfiles/serverside.pem",
            "root_ca_certificate": "tests/testfiles/ca.pem",
            "client_auth": true,
            "server_name_verification": false
          },
        },  
      }"#,
            )
            .unwrap();
        config
    }

    async fn get_basic_router_config_usrpswd() -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec!["tcp/127.0.0.1:7447".parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
        "auth": {
            usrpwd: {
                user: "routername",
                password: "routerpasswd",
                dictionary_file: "tests/testfiles/credentials.txt",
              },
        },
      }"#,
            )
            .unwrap();
        config
    }
    async fn close_router_session(s: Session) {
        println!("Closing router session");
        ztimeout!(s.close().res_async()).unwrap();
    }

    async fn get_client_sessions_tls() -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client(["tls/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                        }
                    }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        let mut config = config::client(["tls/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                        }
                    }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_quic() -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client(["quic/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                        }
                    }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        let mut config = config::client(["quic/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                        }
                    }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_usrpswd() -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client(["tcp/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "auth": {
                            usrpwd: {
                            user: "client1name",
                            password: "client1passwd",
                          },
                        }
                    }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        let mut config = config::client(["tcp/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                    
                        "auth": {
                            usrpwd: {
                            user: "client2name",
                            password: "client2passwd",
                          },
                        
                        }
                    
                    }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config).res_async()).unwrap();
        (s01, s02)
    }

    async fn close_sessions(s01: Session, s02: Session) {
        println!("Closing client sessions");
        ztimeout!(s01.close().res_async()).unwrap();
        ztimeout!(s02.close().res_async()).unwrap();
    }

    async fn test_pub_sub_deny_then_allow_tls() {
        println!("test_pub_sub_deny_then_allow_tls");

        let mut config_router = get_basic_router_config_tls().await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                
              "enabled": false,
              "default_permission": "deny",
              "rules":
              [
                {
                  "permission": "allow",
                  "flows": ["ingress","egress"],
                  "actions": [
                    "put",
                    "declare_subscriber"
                  ],
                  "key_exprs": [
                    "test/demo"
                  ],
                  "cert_common_names": [
                    "client_side"
                  ]
                },
              ]
            }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (sub_session, pub_session) = get_client_sessions_tls().await;
        {
            let publisher = pub_session
                .declare_publisher(KEY_EXPR)
                .res_async()
                .await
                .unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async()
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).res_async().await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow_then_deny_tls() {
        println!("test_pub_sub_allow_then_deny_tls");
        let mut config_router = get_basic_router_config_tls().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
        {"enabled": true,
          "default_permission": "allow",
          "rules":
          [
            {
              "permission": "deny",
              "flows": ["egress"],
              "actions": [
                "put",
                "declare_subscriber"
              ],
              "key_exprs": [
                "test/demo"
              ],
              "cert_common_names": [
                    "client_side"
                  ]
            },
          ]
    }
    "#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
        let (sub_session, pub_session) = get_client_sessions_tls().await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR).res_async()).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = ztimeout!(sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE).res_async()).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny_then_allow_tls() {
        println!("test_get_qbl_deny_then_allow_tls");

        let mut config_router = get_basic_router_config_tls().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
        {"enabled": true,
          "default_permission": "deny",
          "rules":
          [
            {
              "permission": "allow",
              "flows": ["egress","ingress"],
              "actions": [
                "get",
                "declare_queryable"],
              "key_exprs": [
                "test/demo"
              ],
              "cert_common_names": [
                "client_side"
              ]
            },
          ]
    }
    "#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow_then_deny_tls() {
        println!("test_get_qbl_allow_then_deny_tls");

        let mut config_router = get_basic_router_config_tls().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
        {"enabled": true,
          "default_permission": "allow",
          "rules":
          [
            {
              "permission": "deny",
              "flows": ["egress"],
              "actions": [
                "get",
                "declare_queryable" ],
              "key_exprs": [
                "test/demo"
              ],
              "cert_common_names": [
                    "client_side"
                  ]
            },
          ]
    }
    "#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_deny_then_allow_quic() {
        println!("test_pub_sub_deny_then_allow_quic");

        let mut config_router = get_basic_router_config_quic().await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
            
          "enabled": false,
          "default_permission": "deny",
          "rules":
          [
            {
              "permission": "allow",
              "flows": ["ingress","egress"],
              "actions": [
                "put",
                "declare_subscriber"
              ],
              "key_exprs": [
                "test/demo"
              ],
              "cert_common_names": [
                "client_side"
              ]
            },
          ]
        }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (sub_session, pub_session) = get_client_sessions_quic().await;
        {
            let publisher = pub_session
                .declare_publisher(KEY_EXPR)
                .res_async()
                .await
                .unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async()
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).res_async().await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_pub_sub_allow_then_deny_quic() {
        println!("test_pub_sub_allow_then_deny_quic");

        let mut config_router = get_basic_router_config_quic().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "allow",
      "rules":
      [
        {
          "permission": "deny",
          "flows": ["egress"],
          "actions": [
            "put",
            "declare_subscriber"
          ],
          "key_exprs": [
            "test/demo"
          ],
          "cert_common_names": [
                "client_side"
              ]
        },
      ]
}
"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
        let (sub_session, pub_session) = get_client_sessions_quic().await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR).res_async()).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = ztimeout!(sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE).res_async()).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_get_qbl_deny_then_allow_quic() {
        println!("test_get_qbl_deny_then_allow_quic");

        let mut config_router = get_basic_router_config_quic().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "deny",
      "rules":
      [
        {
          "permission": "allow",
          "flows": ["egress","ingress"],
          "actions": [
            "get",
            "declare_queryable"],
          "key_exprs": [
            "test/demo"
          ],
          "cert_common_names": [
            "client_side"
          ]
        },
      ]
}
"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_quic().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_get_qbl_allow_then_deny_quic() {
        println!("test_get_qbl_allow_then_deny_quic");

        let mut config_router = get_basic_router_config_quic().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "allow",
      "rules":
      [
        {
          "permission": "deny",
          "flows": ["egress"],
          "actions": [
            "get",
            "declare_queryable" ],
          "key_exprs": [
            "test/demo"
          ],
          "cert_common_names": [
                "client_side"
              ]
        },
      ]
}
"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_quic().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_deny_then_allow_usrpswd() {
        println!("test_pub_sub_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
            
          "enabled": false,
          "default_permission": "deny",
          "rules":
          [
            {
              "permission": "allow",
              "flows": ["ingress","egress"],
              "actions": [
                "put",
                "declare_subscriber"
              ],
              "key_exprs": [
                "test/demo"
              ],
              "usernames": [
                "client1name","client2name"
              ]
            },
          ]
        }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (sub_session, pub_session) = get_client_sessions_usrpswd().await;
        {
            let publisher = pub_session
                .declare_publisher(KEY_EXPR)
                .res_async()
                .await
                .unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async()
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).res_async().await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow_then_deny_usrpswd() {
        println!("test_pub_sub_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "allow",
      "rules":
      [
        {
          "permission": "deny",
          "flows": ["egress"],
          "actions": [
            "put",
            "declare_subscriber"
          ],
          "key_exprs": [
            "test/demo"
          ],
          "usernames": [
                "client1name","client2name"
              ]
        },
      ]
}
"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
        let (sub_session, pub_session) = get_client_sessions_usrpswd().await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR).res_async()).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = ztimeout!(sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.value.to_string();
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE).res_async()).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare().res_async()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny_then_allow_usrpswd() {
        println!("test_get_qbl_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "deny",
      "rules":
      [
        {
          "permission": "allow",
          "flows": ["egress","ingress"],
          "actions": [
            "get",
            "declare_queryable"],
          "key_exprs": [
            "test/demo"
          ],
          "usernames": [
                "client1name","client2name"
              ]
        },
      ]
}
"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow_then_deny_usrpswd() {
        println!("test_get_qbl_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"
    {"enabled": true,
      "default_permission": "allow",
      "rules":
      [
        {
          "permission": "deny",
          "flows": ["egress"],
          "actions": [
            "get",
            "declare_queryable" ],
          "key_exprs": [
            "test/demo"
          ],
          "usernames": [
                "client1name","client2name"
              ]
        },
      ]
}
"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd().await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap()
                        });
                    });
                })
                .res_async())
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR).res_async()).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.sample {
                    Ok(sample) => {
                        received_value = sample.value.to_string();
                        break;
                    }
                    Err(e) => println!("Error : {}", e),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare().res_async()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }
}
