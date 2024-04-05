use interfaces::Interface;
use std::sync::{Arc, Mutex};
use zenoh::prelude::sync::*;
use zenoh_config::Config;
use zenoh_core::zlock;

#[test]
fn test_acl() {
    env_logger::init();
    test_pub_sub_allow();
    test_pub_sub_deny();
    test_get_queryable_allow();
    test_get_queryable_deny();
    if let Some(loopback_face) = get_loopback_interface() {
        test_pub_sub_allow_then_deny(&loopback_face.name);
        test_pub_sub_deny_then_allow(&loopback_face.name);
        test_get_queryable_allow_then_deny(&loopback_face.name);
        test_get_queryable_deny_then_allow(&loopback_face.name);
    }
}

fn get_loopback_interface() -> Option<Interface> {
    let mut ifs = Interface::get_all().expect("could not get interfaces");
    ifs.sort_by(|a, b| a.name.cmp(&b.name));
    ifs.into_iter().find(|i| i.is_loopback())
}

fn test_pub_sub_deny() {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    config_router
        .insert_json5(
            "transport",
            r#"{
              acl: {
                "enabled": true,
                "default_permission": "deny",
                "rules":
                [
                ]
              }
            }"#,
        )
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
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();

    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();

    let sub_session = zenoh::open(config_sub).res().unwrap();
    let pub_session = zenoh::open(config_pub).res().unwrap();

    let publisher = pub_session.declare_publisher(KEY_EXPR).res().unwrap();

    let received_value = Arc::new(Mutex::new(String::new()));
    let temp_recv_value = received_value.clone();

    let _subscriber = &sub_session
        .declare_subscriber(KEY_EXPR)
        .callback(move |sample| {
            let mut temp_value = zlock!(temp_recv_value);
            *temp_value = sample.value.to_string();
        })
        .res()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert_ne!(*zlock!(received_value), VALUE);
}

fn test_pub_sub_allow() {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    config_router
        .insert_json5(
            "transport",
            r#"{
            acl: {
              "enabled": true,
              "default_permission": "allow",
              "rules":
              [
              ]
            }
          }"#,
        )
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
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();

    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();

    let sub_session = zenoh::open(config_sub).res().unwrap();
    let pub_session = zenoh::open(config_pub).res().unwrap();

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

    std::thread::sleep(std::time::Duration::from_millis(10));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert_eq!(*zlock!(received_value), VALUE);
}

fn test_pub_sub_allow_then_deny(interface_name: &str) {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let acl_js = format!(
        r#"
        {{
        acl: {{
          "enabled": true,
          "default_permission": "allow",
          "rules":
          [
            {{
              "permission": "deny",
              "flow": ["egress"],
              "action": [
                "put",
              ],
              "key_expr": [
                "test/demo"
              ],
              "interface": [
                "{}"
              ]
            }},
          ]
        }}
    }}
    "#,
        interface_name
    );
    config_router.insert_json5("transport", &acl_js).unwrap();
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
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();

    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();

    let sub_session = zenoh::open(config_sub).res().unwrap();
    let pub_session = zenoh::open(config_pub).res().unwrap();

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

    std::thread::sleep(std::time::Duration::from_millis(10));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert_ne!(*zlock!(received_value), VALUE);
}
fn test_pub_sub_deny_then_allow(interface_name: &str) {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let acl_js = format!(
        r#"
        {{
        acl: {{
          "enabled": true,
          "default_permission": "deny",
          "rules":
          [
            {{
              "permission": "allow",
              "flow": ["egress","ingress"],
              "action": [
                "put",
                "declare_subscriber"
              ],
              "key_expr": [
                "test/demo"
              ],
              "interface": [
                "{}"
              ]
            }},
          ]
        }}
    }}
    "#,
        interface_name
    );
    config_router.insert_json5("transport", &acl_js).unwrap();

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
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();
    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";

    let _session = zenoh::open(config_router).res().unwrap();

    let sub_session = zenoh::open(config_sub).res().unwrap();
    let pub_session = zenoh::open(config_pub).res().unwrap();

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

    std::thread::sleep(std::time::Duration::from_millis(10));
    publisher.put(VALUE).res().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert_eq!(*zlock!(received_value), VALUE);
}

fn test_get_queryable_deny() {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    config_router
        .insert_json5(
            "transport",
            r#"{
          acl: {
            "enabled": true,
            "default_permission": "deny",
            "rules":
            [
              
            ]
          }
        }"#,
        )
        .unwrap();
    let mut config_qbl = Config::default();
    config_qbl.set_mode(Some(WhatAmI::Client)).unwrap();
    config_qbl
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_qbl
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let mut config_get = Config::default();
    config_get.set_mode(Some(WhatAmI::Client)).unwrap();
    config_get
        .connect
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();
    config_get
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl).res().unwrap();
    let get_session = zenoh::open(config_get).res().unwrap();
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

fn test_get_queryable_allow() {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    config_router
        .insert_json5(
            "transport",
            r#"{
          acl: {
            "enabled": true,
            "default_permission": "allow",
            "rules":
            [
              
            ]
          }
        }"#,
        )
        .unwrap();
    let mut config_qbl = Config::default();
    config_qbl.set_mode(Some(WhatAmI::Client)).unwrap();
    config_qbl
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_qbl
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let mut config_get = Config::default();
    config_get.set_mode(Some(WhatAmI::Client)).unwrap();
    config_get
        .connect
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();
    config_get
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl).res().unwrap();
    let get_session = zenoh::open(config_get).res().unwrap();
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

fn test_get_queryable_allow_then_deny(interface_name: &str) {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let acl_js = format!(
        r#"
        {{
        acl: {{
          "enabled": true,
          "default_permission": "allow",
          "rules":
          [
            {{
              "permission": "deny",
              "flow": ["egress"],
              "action": [
                "get",
                "declare_queryable"
              ],
              "key_expr": [
                "test/demo"
              ],
              "interface": [
                "{}"
              ]
            }},
          ]
        }}
    }}
    "#,
        interface_name
    );
    config_router.insert_json5("transport", &acl_js).unwrap();
    let mut config_qbl = Config::default();
    config_qbl.set_mode(Some(WhatAmI::Client)).unwrap();
    config_qbl
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_qbl
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let mut config_get = Config::default();
    config_get.set_mode(Some(WhatAmI::Client)).unwrap();
    config_get
        .connect
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();
    config_get
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl).res().unwrap();
    let get_session = zenoh::open(config_get).res().unwrap();
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

fn test_get_queryable_deny_then_allow(interface_name: &str) {
    let mut config_router = Config::default();
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    config_router
        .listen
        .set_endpoints(vec![
            "tcp/localhost:7447".parse().unwrap(),
            "tcp/localhost:7448".parse().unwrap(),
        ])
        .unwrap();

    config_router
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let acl_js = format!(
        r#"
        {{
        acl: {{
          "enabled": true,
          "default_permission": "deny",
          "rules":
          [
            {{
              "permission": "allow",
              "flow": ["egress","ingress"],
              "action": [
                "get",
                "declare_queryable"
              ],
              "key_expr": [
                "test/demo"
              ],
              "interface": [
                "{}"
              ]
            }},
          ]
        }}
    }}
    "#,
        interface_name
    );
    config_router.insert_json5("transport", &acl_js).unwrap();
    let mut config_qbl = Config::default();
    config_qbl.set_mode(Some(WhatAmI::Client)).unwrap();
    config_qbl
        .connect
        .set_endpoints(vec!["tcp/localhost:7447".parse().unwrap()])
        .unwrap();
    config_qbl
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let mut config_get = Config::default();
    config_get.set_mode(Some(WhatAmI::Client)).unwrap();
    config_get
        .connect
        .set_endpoints(vec!["tcp/localhost:7448".parse().unwrap()])
        .unwrap();
    config_get
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    let _session = zenoh::open(config_router).res().unwrap();
    let qbl_session = zenoh::open(config_qbl).res().unwrap();
    let get_session = zenoh::open(config_get).res().unwrap();
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
