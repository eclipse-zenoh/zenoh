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

use crate::net::primitives::{DummyPrimitives, EPrimitives, Primitives};
use crate::net::routing::dispatcher::tables::{self, Tables};
use crate::net::routing::router::*;
use crate::net::routing::RoutingContext;
use std::convert::{TryFrom, TryInto};
use std::sync::Arc;
use uhlc::HLC;
use zenoh_buffers::ZBuf;
use zenoh_config::Config;
use zenoh_core::zlock;
use zenoh_protocol::core::Encoding;
use zenoh_protocol::core::{
    key_expr::keyexpr, ExprId, Reliability, WhatAmI, WireExpr, ZenohId, EMPTY_EXPR_ID,
};
use zenoh_protocol::network::declare::subscriber::ext::SubscriberInfo;
use zenoh_protocol::network::declare::Mode;
use zenoh_protocol::network::{ext, Declare, DeclareBody, DeclareKeyExpr};
use zenoh_protocol::zenoh::{PushBody, Put};

#[test]
fn base_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let primitives = Arc::new(DummyPrimitives {});
    let face = Arc::downgrade(&router.new_primitives(primitives).state);
    register_expr(
        &tables,
        &mut face.upgrade().unwrap(),
        1,
        &"one/two/three".into(),
    );
    register_expr(
        &tables,
        &mut face.upgrade().unwrap(),
        2,
        &"one/deux/trois".into(),
    );

    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable,
        mode: Mode::Push,
    };

    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("four/five"),
        &sub_info,
        NodeId::default(),
    );

    Tables::print(&zread!(tables.tables));
}

#[test]
fn match_test() {
    let key_exprs = [
        "**",
        "a",
        "a/b",
        "*",
        "a/*",
        "a/b$*",
        "abc",
        "xx",
        "ab$*",
        "abcd",
        "ab$*d",
        "ab",
        "ab/*",
        "a/*/c/*/e",
        "a/b/c/d/e",
        "a/$*b/c/$*d/e",
        "a/xb/c/xd/e",
        "a/c/e",
        "a/b/c/d/x/e",
        "ab$*cd",
        "abxxcxxd",
        "abxxcxxcd",
        "abxxcxxcdx",
        "a/b/c",
        "ab/**",
        "**/xyz",
        "a/b/xyz/d/e/f/xyz",
        "**/xyz$*xyz",
        "a/b/xyz/d/e/f/xyz",
        "a/**/c/**/e",
        "a/b/b/b/c/d/d/d/e",
        "a/**/c/*/e/*",
        "a/b/b/b/c/d/d/c/d/e/f",
        "a/**/c/*/e/*",
        "x/abc",
        "x/*",
        "x/abc$*",
        "x/$*abc",
        "x/a$*",
        "x/a$*de",
        "x/abc$*de",
        "x/a$*d$*e",
        "x/a$*e",
        "x/a$*c$*e",
        "x/ade",
        "x/c$*",
        "x/$*d",
        "x/$*e",
    ]
    .map(|s| keyexpr::new(s).unwrap());

    let config = Config::default();
    let router = Router::new(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let primitives = Arc::new(DummyPrimitives {});
    let face = Arc::downgrade(&router.new_primitives(primitives).state);
    for (i, key_expr) in key_exprs.iter().enumerate() {
        register_expr(
            &tables,
            &mut face.upgrade().unwrap(),
            i.try_into().unwrap(),
            &(*key_expr).into(),
        );
    }

    for key_expr1 in key_exprs.iter() {
        let res_matches = Resource::get_matches(&zread!(tables.tables), key_expr1);
        dbg!(res_matches.len());
        for key_expr2 in key_exprs.iter() {
            if res_matches
                .iter()
                .map(|m| m.upgrade().unwrap().expr())
                .any(|x| x.as_str() == key_expr2.as_str())
            {
                assert!(dbg!(dbg!(key_expr1).intersects(dbg!(key_expr2))));
            } else {
                assert!(!dbg!(dbg!(key_expr1).intersects(dbg!(key_expr2))));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn clean_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let primitives = Arc::new(DummyPrimitives {});
    let face0 = Arc::downgrade(&router.new_primitives(primitives).state);
    assert!(face0.upgrade().is_some());

    // --------------
    register_expr(&tables, &mut face0.upgrade().unwrap(), 1, &"todrop1".into());
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        2,
        &"todrop1/todrop11".into(),
    );
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    register_expr(&tables, &mut face0.upgrade().unwrap(), 3, &"**".into());
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "**")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 1);
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 2);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 3);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.upgrade().unwrap(), 1, &"todrop1".into());
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable,
        mode: Mode::Push,
    };

    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop1/todrop11".into(),
        &sub_info,
        NodeId::default(),
    );
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("/todrop12"),
        &sub_info,
        NodeId::default(),
    );
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop12")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    println!("COUNT: {}", res3.strong_count());
    assert!(res3.upgrade().is_some());

    undeclare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("/todrop12"),
        NodeId::default(),
    );

    println!("COUNT2: {}", res3.strong_count());

    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_none());

    undeclare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop1/todrop11".into(),
        NodeId::default(),
    );
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 1);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.upgrade().unwrap(), 2, &"todrop3".into());
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop3".into(),
        &sub_info,
        NodeId::default(),
    );
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop3")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    undeclare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop3".into(),
        NodeId::default(),
    );
    assert!(res1.upgrade().is_some());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 2);
    assert!(res1.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.upgrade().unwrap(), 3, &"todrop4".into());
    register_expr(&tables, &mut face0.upgrade().unwrap(), 4, &"todrop5".into());
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop5".into(),
        &sub_info,
        NodeId::default(),
    );
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &"todrop6".into(),
        &sub_info,
        NodeId::default(),
    );

    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop4")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop5")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop6")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();

    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    tables::close_face(&tables, &face0);
    assert!(face0.upgrade().is_none());
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());
}

pub struct ClientPrimitives {
    data: std::sync::Mutex<Option<WireExpr<'static>>>,
    mapping: std::sync::Mutex<std::collections::HashMap<ExprId, String>>,
}

impl ClientPrimitives {
    pub fn new() -> ClientPrimitives {
        ClientPrimitives {
            data: std::sync::Mutex::new(None),
            mapping: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn clear_data(&self) {
        *self.data.lock().unwrap() = None;
    }
}

impl Default for ClientPrimitives {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientPrimitives {
    fn get_name(&self, key_expr: &WireExpr) -> String {
        let mapping = self.mapping.lock().unwrap();
        let (scope, suffix) = key_expr.as_id_and_suffix();
        if scope == EMPTY_EXPR_ID {
            suffix.to_string()
        } else if suffix.is_empty() {
            mapping.get(&scope).unwrap().clone()
        } else {
            format!("{}{}", mapping.get(&scope).unwrap(), suffix)
        }
    }

    fn get_last_name(&self) -> Option<String> {
        self.data
            .lock()
            .unwrap()
            .as_ref()
            .map(|data| self.get_name(data))
    }

    #[allow(dead_code)]
    fn get_last_key(&self) -> Option<WireExpr> {
        self.data.lock().unwrap().as_ref().cloned()
    }
}

impl Primitives for ClientPrimitives {
    fn send_declare(&self, msg: zenoh_protocol::network::Declare) {
        match msg.body {
            DeclareBody::DeclareKeyExpr(d) => {
                let name = self.get_name(&d.wire_expr);
                zlock!(self.mapping).insert(d.id, name);
            }
            DeclareBody::UndeclareKeyExpr(u) => {
                zlock!(self.mapping).remove(&u.id);
            }
            _ => (),
        }
    }

    fn send_push(&self, msg: zenoh_protocol::network::Push) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_request(&self, _msg: zenoh_protocol::network::Request) {}

    fn send_response(&self, _msg: zenoh_protocol::network::Response) {}

    fn send_response_final(&self, _msg: zenoh_protocol::network::ResponseFinal) {}

    fn send_close(&self) {}
}

impl EPrimitives for ClientPrimitives {
    fn send_declare(&self, ctx: RoutingContext<zenoh_protocol::network::Declare>) {
        match ctx.msg.body {
            DeclareBody::DeclareKeyExpr(d) => {
                let name = self.get_name(&d.wire_expr);
                zlock!(self.mapping).insert(d.id, name);
            }
            DeclareBody::UndeclareKeyExpr(u) => {
                zlock!(self.mapping).remove(&u.id);
            }
            _ => (),
        }
    }

    fn send_push(&self, msg: zenoh_protocol::network::Push) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_request(&self, _ctx: RoutingContext<zenoh_protocol::network::Request>) {}

    fn send_response(&self, _ctx: RoutingContext<zenoh_protocol::network::Response>) {}

    fn send_response_final(&self, _ctx: RoutingContext<zenoh_protocol::network::ResponseFinal>) {}

    fn send_close(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn client_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable,
        mode: Mode::Push,
    };

    let primitives0 = Arc::new(ClientPrimitives::new());
    let face0 = Arc::downgrade(&router.new_primitives(primitives0.clone()).state);
    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        11,
        &"test/client".into(),
    );
    Primitives::send_declare(
        primitives0.as_ref(),
        Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 11,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(11).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
    );
    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        12,
        &WireExpr::from(11).with_suffix("/z1_pub1"),
    );
    Primitives::send_declare(
        primitives0.as_ref(),
        Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 12,
                wire_expr: WireExpr::from(11).with_suffix("/z1_pub1"),
            }),
        },
    );

    let primitives1 = Arc::new(ClientPrimitives::new());
    let face1 = Arc::downgrade(&router.new_primitives(primitives1.clone()).state);
    register_expr(
        &tables,
        &mut face1.upgrade().unwrap(),
        21,
        &"test/client".into(),
    );
    Primitives::send_declare(
        primitives1.as_ref(),
        Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 21,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face1.upgrade().unwrap(),
        &WireExpr::from(21).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
    );
    register_expr(
        &tables,
        &mut face1.upgrade().unwrap(),
        22,
        &WireExpr::from(21).with_suffix("/z2_pub1"),
    );
    Primitives::send_declare(
        primitives1.as_ref(),
        Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 22,
                wire_expr: WireExpr::from(21).with_suffix("/z2_pub1"),
            }),
        },
    );

    let primitives2 = Arc::new(ClientPrimitives::new());
    let face2 = Arc::downgrade(&router.new_primitives(primitives2.clone()).state);
    register_expr(
        &tables,
        &mut face2.upgrade().unwrap(),
        31,
        &"test/client".into(),
    );
    Primitives::send_declare(
        primitives2.as_ref(),
        Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 31,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        zlock!(tables.ctrl_lock).as_ref(),
        &tables,
        &mut face2.upgrade().unwrap(),
        &WireExpr::from(31).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
    );

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();

    full_reentrant_route_data(
        &tables,
        &face0.upgrade().unwrap(),
        &"test/client/z1_wr1".into(),
        ext::QoSType::default(),
        PushBody::Put(Put {
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
            payload: ZBuf::empty(),
            ext_attachment: None,
        }),
        0,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_wr1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_wr1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    full_reentrant_route_data(
        &router.tables,
        &face0.upgrade().unwrap(),
        &WireExpr::from(11).with_suffix("/z1_wr2"),
        ext::QoSType::default(),
        PushBody::Put(Put {
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
            payload: ZBuf::empty(),
            ext_attachment: None,
        }),
        0,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_wr2".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_wr2".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    full_reentrant_route_data(
        &router.tables,
        &face1.upgrade().unwrap(),
        &"test/client/**".into(),
        ext::QoSType::default(),
        PushBody::Put(Put {
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
            payload: ZBuf::empty(),
            ext_attachment: None,
        }),
        0,
    );

    // functionnal check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "test/client/**");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(11, "/**".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/**");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/**".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    full_reentrant_route_data(
        &router.tables,
        &face0.upgrade().unwrap(),
        &12.into(),
        ext::QoSType::default(),
        PushBody::Put(Put {
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
            payload: ZBuf::empty(),
            ext_attachment: None,
        }),
        0,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_pub1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_pub1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    full_reentrant_route_data(
        &router.tables,
        &face1.upgrade().unwrap(),
        &22.into(),
        ext::QoSType::default(),
        PushBody::Put(Put {
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
            payload: ZBuf::empty(),
            ext_attachment: None,
        }),
        0,
    );

    // functionnal check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(11, "/z2_pub1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z2_pub1".to_string()));
}
