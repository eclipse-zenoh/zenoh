use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh_core::{zlock, ztimeout};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const KEY_EXPR: &str = "test/demo";
const VALUE: &str = "zenoh";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(not(target_os = "windows"))]
async fn test_acl() {
    env_logger::init();
    test_pub_sub_allow_then_deny().await;
    test_pub_sub_deny_then_allow().await;
    test_pub_sub_allow_then_deny().await;
    test_pub_sub_deny().await;
    test_pub_sub_allow().await;
}

async fn get_basic_router_config() -> Config {
    let mut config = config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config.listen.endpoints = vec!["tcp/127.0.0.1:7447".parse().unwrap()];
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config
}

async fn close_router_session(s: Session) {
    println!("[  ][01d] Closing router session");
    ztimeout!(s.close().res_async()).unwrap();
}

async fn get_client_sessions() -> (Session, Session) {
    // Open the sessions
    let config = config::client(["tcp/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
    println!("[  ][01a] Opening read session");
    let s01 = ztimeout!(zenoh::open(config).res_async()).unwrap();
    let config = config::client(["tcp/127.0.0.1:7447".parse::<EndPoint>().unwrap()]);
    println!("[  ][01a] Opening write session");
    let s02 = ztimeout!(zenoh::open(config).res_async()).unwrap();
    (s01, s02)
}

async fn close_sessions(s01: Session, s02: Session) {
    println!("[  ][01d] Closing read session");
    ztimeout!(s01.close().res_async()).unwrap();
    println!("[  ][02d] Closing write session");
    ztimeout!(s02.close().res_async()).unwrap();
}

async fn test_pub_sub_deny() {
    println!("test_pub_sub_deny");

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "acl",
            r#"{
                "enabled": true,
                "default_permission": "deny",
                "rules":
                [
                ]
            }"#,
        )
        .unwrap();
    println!(" Opening router session");

    let _session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();

    let (sub_session, pub_session) = get_client_sessions().await;
    {
        let publisher = pub_session
            .declare_publisher(KEY_EXPR)
            .res_async()
            .await
            .unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let temp_recv_value = received_value.clone();
        let _subscriber = sub_session
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

        assert_ne!(*zlock!(received_value), VALUE);
    }
    close_sessions(sub_session, pub_session).await;
    close_router_session(_session).await;
}

async fn test_pub_sub_allow() {
    println!("test_pub_sub_allow");
    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "acl",
            r#"{
            
              "enabled": false,
              "default_permission": "allow",
              "rules":
              [
              ]
            
          }"#,
        )
        .unwrap();
    println!(" Opening router session");

    let _session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let (sub_session, pub_session) = get_client_sessions().await;
    {
        let publisher = pub_session
            .declare_publisher(KEY_EXPR)
            .res_async()
            .await
            .unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let temp_recv_value = received_value.clone();
        let _subscriber = sub_session
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
    }
    close_sessions(sub_session, pub_session).await;
    close_router_session(_session).await;
}

async fn test_pub_sub_allow_then_deny() {
    println!("test_pub_sub_allow_then_deny");

    let mut config_router = get_basic_router_config().await;

    let mut config_router = config_router.clone();
    config_router
        .insert_json5(
            "acl",
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
              "interfaces": [
                "lo","lo0"
              ]
            },
          ]
    }
    "#,
        )
        .unwrap();
    println!(" Opening router session");

    let _session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let (sub_session, pub_session) = get_client_sessions().await;
    {
        let publisher = pub_session
            .declare_publisher(KEY_EXPR)
            .res_async()
            .await
            .unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let temp_recv_value = received_value.clone();
        let _subscriber = sub_session
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

        assert_ne!(*zlock!(received_value), VALUE);
    }
    close_sessions(sub_session, pub_session).await;
    close_router_session(_session).await;
}

async fn test_pub_sub_deny_then_allow() {
    println!("test_pub_sub_deny_then_allow");

    let mut config_router = get_basic_router_config().await;
    config_router
        .insert_json5(
            "acl",
            r#"
        {"enabled": true,
          "default_permission": "deny",
          "rules":
          [
            {
              "permission": "allow",
              "flows": ["egress","ingress"],
              "actions": [
                "put",
                "declare_subscriber"
              ],
              "key_exprs": [
                "test/demo"
              ],
              "interfaces": [
                "lo","lo0"
              ]
            },
          ]
    }
    "#,
        )
        .unwrap();
    println!(" Opening router session");

    let _session = ztimeout!(zenoh::open(config_router).res_async()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let (sub_session, pub_session) = get_client_sessions().await;
    {
        let publisher = pub_session
            .declare_publisher(KEY_EXPR)
            .res_async()
            .await
            .unwrap();
        let received_value = Arc::new(Mutex::new(String::new()));
        let temp_recv_value = received_value.clone();
        let _subscriber = sub_session
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
    }
    close_sessions(sub_session, pub_session).await;
    close_router_session(_session).await;
}
