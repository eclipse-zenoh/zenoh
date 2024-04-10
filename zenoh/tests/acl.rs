use std::sync::{Arc, Mutex};
use zenoh::prelude::sync::*;
use zenoh_config::Config;
use zenoh_core::zlock;
#[test]
#[cfg(not(target_os = "windows"))]
fn test_acl() {
    env_logger::init();

    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();

    let mut config_sub = Config::default();
    config_sub.set_mode(Some(WhatAmI::Client)).unwrap();
    config_sub
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_sub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    let mut config_pub = Config::default();
    config_pub.set_mode(Some(WhatAmI::Client)).unwrap();
    config_pub
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    let config_qbl = &config_pub;
    let config_get = &config_sub;

    test_pub_sub_allow(&config_router, &config_sub, &config_pub);
    test_pub_sub_deny(&config_router, &config_sub, &config_pub);
    test_get_queryable_allow(&config_router, config_qbl, config_get);
    test_get_queryable_deny(&config_router, config_qbl, config_get);
    test_pub_sub_allow_then_deny(&config_router, &config_sub, &config_pub);
    test_pub_sub_deny_then_allow(&config_router, &config_sub, &config_pub);
    test_get_queryable_allow_then_deny(&config_router, config_qbl, config_get);
    test_get_queryable_deny_then_allow(&config_router, config_qbl, config_get);
}

fn test_pub_sub_deny(config_router: &Config, config_pub: &Config, config_sub: &Config) {
    let mut config_router = config_router.clone();
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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let sub_session = zenoh::open(config_sub.clone()).res().unwrap();
    let pub_session = zenoh::open(config_pub.clone()).res().unwrap();
    let publisher = pub_session.declare_publisher(KEY_EXPR).res().unwrap();
    let received_value = Arc::new(Mutex::new(String::new()));
    let temp_recv_value = received_value.clone();
    let _subscriber = sub_session
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            let mut temp_value = zlock!(temp_recv_value);
            *temp_value = sample.value.to_string();
        })
        .res()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_ne!(*zlock!(received_value), VALUE);
}

fn test_pub_sub_allow(config_router: &Config, config_pub: &Config, config_sub: &Config) {
    let mut config_router = config_router.clone();
    config_router
        .insert_json5(
            "acl",
            r#"{
            
              "enabled": true,
              "default_permission": "allow",
              "rules":
              [
              ]
            
          }"#,
        )
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();
    let sub_session = zenoh::open(config_sub.clone()).res().unwrap();
    let pub_session = zenoh::open(config_pub.clone()).res().unwrap();
    let publisher = pub_session.declare_publisher(KEY_EXPR).res().unwrap();
    let received_value = Arc::new(Mutex::new(String::new()));
    let temp_recv_value = received_value.clone();
    let _subscriber = sub_session
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            let mut temp_value = zlock!(temp_recv_value);
            *temp_value = sample.value.to_string();
        })
        .res()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_eq!(*zlock!(received_value), VALUE);
}

fn test_pub_sub_allow_then_deny(config_router: &Config, config_pub: &Config, config_sub: &Config) {
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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let sub_session = zenoh::open(config_sub.clone()).res().unwrap();
    let pub_session = zenoh::open(config_pub.clone()).res().unwrap();
    let publisher = pub_session.declare_publisher(KEY_EXPR).res().unwrap();
    let received_value = Arc::new(Mutex::new(String::new()));
    let temp_recv_value = received_value.clone();
    let _subscriber = sub_session
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            let mut temp_value = zlock!(temp_recv_value);
            *temp_value = sample.value.to_string();
        })
        .res()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_ne!(*zlock!(received_value), VALUE);
}

fn test_pub_sub_deny_then_allow(config_router: &Config, config_pub: &Config, config_sub: &Config) {
    let mut config_router = config_router.clone();
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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();
    let sub_session = zenoh::open(config_sub.clone()).res().unwrap();
    let pub_session = zenoh::open(config_pub.clone()).res().unwrap();
    let publisher = pub_session.declare_publisher(KEY_EXPR).res().unwrap();
    let received_value = Arc::new(Mutex::new(String::new()));
    let temp_recv_value = received_value.clone();
    let _subscriber = sub_session
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            let mut temp_value = zlock!(temp_recv_value);
            *temp_value = sample.value.to_string();
        })
        .res()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_eq!(*zlock!(received_value), VALUE);
}

fn test_get_queryable_deny(config_router: &Config, config_qbl: &Config, config_get: &Config) {
    let mut config_router = config_router.clone();

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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl.clone()).res().unwrap();
    let get_session = zenoh::open(config_get.clone()).res().unwrap();
    let mut received_value = String::new();
    let _qbl = qbl_session
        .declare_queryable(KEY_EXPR)
        .callback(move |sample| {
            let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
            sample.reply(Ok(rep)).res().unwrap();
        })
        .res()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let recv_reply = get_session.get(KEY_EXPR).res().unwrap();
    while let Ok(reply) = recv_reply.recv() {
        match reply.sample {
            Ok(sample) => {
                received_value = sample.value.to_string();
                break;
            }
            Err(e) => println!("Error : {}", e),
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_ne!(received_value, VALUE);
}

fn test_get_queryable_allow(config_router: &Config, config_qbl: &Config, config_get: &Config) {
    let mut config_router = config_router.clone();

    config_router
        .insert_json5(
            "acl",
            r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules":
            [
            ]
        }"#,
        )
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl.clone()).res().unwrap();
    let get_session = zenoh::open(config_get.clone()).res().unwrap();
    let mut received_value = String::new();
    let _qbl = qbl_session
        .declare_queryable(KEY_EXPR)
        .callback(move |sample| {
            let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
            sample.reply(Ok(rep)).res().unwrap();
        })
        .res()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let recv_reply = get_session.get(KEY_EXPR).res().unwrap();
    while let Ok(reply) = recv_reply.recv() {
        match reply.sample {
            Ok(sample) => {
                received_value = sample.value.to_string();
                break;
            }
            Err(e) => println!("Error : {}", e),
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_eq!(received_value, VALUE);
}

fn test_get_queryable_allow_then_deny(
    config_router: &Config,
    config_qbl: &Config,
    config_get: &Config,
) {
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
                "get",
                "declare_queryable"              ],
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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl.clone()).res().unwrap();
    let get_session = zenoh::open(config_get.clone()).res().unwrap();
    let mut received_value = String::new();
    let _qbl = qbl_session
        .declare_queryable(KEY_EXPR)
        .callback(move |sample| {
            let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
            sample.reply(Ok(rep)).res().unwrap();
        })
        .res()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let recv_reply = get_session.get(KEY_EXPR).res().unwrap();
    while let Ok(reply) = recv_reply.recv() {
        match reply.sample {
            Ok(sample) => {
                received_value = sample.value.to_string();
                break;
            }
            Err(e) => println!("Error : {}", e),
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_ne!(received_value, VALUE);
}

fn test_get_queryable_deny_then_allow(
    config_router: &Config,
    config_qbl: &Config,
    config_get: &Config,
) {
    let mut config_router = config_router.clone();
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
                "get",
                "declare_queryable"              ],
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

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl.clone()).res().unwrap();
    let get_session = zenoh::open(config_get.clone()).res().unwrap();
    let mut received_value = String::new();
    let _qbl = qbl_session
        .declare_queryable(KEY_EXPR)
        .callback(move |sample| {
            let rep = Sample::try_from(KEY_EXPR, VALUE).unwrap();
            sample.reply(Ok(rep)).res().unwrap();
        })
        .res()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let recv_reply = get_session.get(KEY_EXPR).res().unwrap();
    while let Ok(reply) = recv_reply.recv() {
        match reply.sample {
            Ok(sample) => {
                received_value = sample.value.to_string();
                break;
            }
            Err(e) => println!("Error : {}", e),
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_eq!(received_value, VALUE);
}
