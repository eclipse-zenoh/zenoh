//
// Copyright (c) 2023 ZettaScale Technology
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
use std::{
    convert::{TryFrom, TryInto},
    sync::{Arc, Weak},
};

use uhlc::HLC;
use zenoh_config::Config;
use zenoh_core::zlock;
use zenoh_protocol::{
    core::{
        key_expr::keyexpr, ExprId, Reliability, WhatAmI, WireExpr, ZenohIdProto, EMPTY_EXPR_ID,
    },
    network::{ext, Declare, DeclareBody, DeclareKeyExpr, Push},
    zenoh::Put,
};

use crate::{
    key_expr::KeyExpr,
    net::{
        primitives::{DummyPrimitives, EPrimitives, Primitives},
        routing::{
            dispatcher::{
                face::{Face, FaceState},
                pubsub::SubscriberInfo,
                tables::Tables,
            },
            router::*,
            RoutingContext,
        },
    },
};

#[test]
fn base_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
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

    let sub_info = SubscriberInfo;

    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face.upgrade().unwrap(),
        0,
        &WireExpr::from(1).with_suffix("four/five"),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
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
        ZenohIdProto::try_from([1]).unwrap(),
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
                .any(|m| m.upgrade().unwrap().expr() == key_expr2.as_str())
            {
                assert!(dbg!(dbg!(key_expr1).intersects(dbg!(key_expr2))));
            } else {
                assert!(!dbg!(dbg!(key_expr1).intersects(dbg!(key_expr2))));
            }
        }
    }
}

#[test]
fn multisub_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let primitives = Arc::new(DummyPrimitives {});
    let face0 = &router.new_primitives(primitives);

    // --------------
    let sub_info = SubscriberInfo;
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        0,
        &"sub".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    let optres = Resource::get_resource(zread!(tables.tables)._get_root(), "sub")
        .map(|res| Arc::downgrade(&res));
    assert!(optres.is_some());
    let res = optres.unwrap();
    assert!(res.upgrade().is_some());

    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        1,
        &"sub".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    assert!(res.upgrade().is_some());

    undeclare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        0,
        &WireExpr::empty(),
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    assert!(res.upgrade().is_some());

    undeclare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        1,
        &WireExpr::empty(),
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    assert!(res.upgrade().is_none());

    face0.send_close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn clean_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let primitives = Arc::new(DummyPrimitives {});
    let face0 = &router.new_primitives(primitives);

    // --------------
    register_expr(&tables, &mut face0.state.clone(), 1, &"todrop1".into());
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    register_expr(
        &tables,
        &mut face0.state.clone(),
        2,
        &"todrop1/todrop11".into(),
    );
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    register_expr(&tables, &mut face0.state.clone(), 3, &"**".into());
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "**")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.state.clone(), 1);
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.state.clone(), 2);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_some());

    unregister_expr(&tables, &mut face0.state.clone(), 3);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.state.clone(), 1, &"todrop1".into());
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    let sub_info = SubscriberInfo;

    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        0,
        &"todrop1/todrop11".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        1,
        &WireExpr::from(1).with_suffix("/todrop12"),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop12")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    println!("COUNT: {}", res3.strong_count());
    assert!(res3.upgrade().is_some());

    undeclare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        1,
        &WireExpr::empty(),
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );

    println!("COUNT2: {}", res3.strong_count());

    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_none());

    undeclare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        0,
        &WireExpr::empty(),
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    unregister_expr(&tables, &mut face0.state.clone(), 1);
    assert!(res1.upgrade().is_none());
    assert!(res2.upgrade().is_none());
    assert!(res3.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.state.clone(), 2, &"todrop3".into());
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        2,
        &"todrop3".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop3")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    undeclare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        2,
        &WireExpr::empty(),
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    assert!(res1.upgrade().is_some());

    unregister_expr(&tables, &mut face0.state.clone(), 2);
    assert!(res1.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.state.clone(), 3, &"todrop4".into());
    register_expr(&tables, &mut face0.state.clone(), 4, &"todrop5".into());
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        3,
        &"todrop5".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.state.clone(),
        4,
        &"todrop6".into(),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
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

    face0.send_close();
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
    fn get_last_key(&self) -> Option<WireExpr<'_>> {
        self.data.lock().unwrap().as_ref().cloned()
    }
}

impl Primitives for ClientPrimitives {
    fn send_interest(&self, _msg: &mut zenoh_protocol::network::Interest) {}

    fn send_declare(&self, msg: &mut zenoh_protocol::network::Declare) {
        match &msg.body {
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

    fn send_push(&self, msg: &mut zenoh_protocol::network::Push, _reliability: Reliability) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_request(&self, _msg: &mut zenoh_protocol::network::Request) {}

    fn send_response(&self, _msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, _msg: &mut zenoh_protocol::network::ResponseFinal) {}

    fn send_close(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl EPrimitives for ClientPrimitives {
    fn send_interest(&self, _ctx: RoutingContext<&mut zenoh_protocol::network::Interest>) {}

    fn send_declare(&self, ctx: RoutingContext<&mut zenoh_protocol::network::Declare>) {
        match &ctx.msg.body {
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

    fn send_push(&self, msg: &mut zenoh_protocol::network::Push, _reliability: Reliability) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_request(&self, msg: &mut zenoh_protocol::network::Request) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_response(&self, msg: &mut zenoh_protocol::network::Response) {
        *zlock!(self.data) = Some(msg.wire_expr.to_owned());
    }

    fn send_response_final(&self, _msg: &mut zenoh_protocol::network::ResponseFinal) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn client_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
        WhatAmI::Client,
        Some(Arc::new(HLC::default())),
        &config,
    )
    .unwrap();
    let tables = router.tables.clone();

    let sub_info = SubscriberInfo;

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
        &mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 11,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face0.upgrade().unwrap(),
        0,
        &WireExpr::from(11).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        12,
        &WireExpr::from(11).with_suffix("/z1_pub1"),
    );
    Primitives::send_declare(
        primitives0.as_ref(),
        &mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
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
        &mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 21,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face1.upgrade().unwrap(),
        0,
        &WireExpr::from(21).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );
    register_expr(
        &tables,
        &mut face1.upgrade().unwrap(),
        22,
        &WireExpr::from(21).with_suffix("/z2_pub1"),
    );
    Primitives::send_declare(
        primitives1.as_ref(),
        &mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
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
        &mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id: 31,
                wire_expr: "test/client".into(),
            }),
        },
    );
    declare_subscription(
        tables.hat_code.as_ref(),
        &tables,
        &mut face2.upgrade().unwrap(),
        0,
        &WireExpr::from(31).with_suffix("/**"),
        &sub_info,
        NodeId::default(),
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();

    let route_dummy_data = |face: &Weak<FaceState>, wire_expr| {
        route_data(
            &tables,
            &face.upgrade().unwrap(),
            &mut Push {
                wire_expr,
                ..Put::default().into()
            },
            Reliability::Reliable,
        );
    };

    route_dummy_data(&face0, "test/client/z1_wr1".into());

    // functional check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_wr1".to_string()));

    // functional check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_wr1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_dummy_data(&face0, WireExpr::from(11).with_suffix("/z1_wr2"));

    // functional check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_wr2".to_string()));

    // functional check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_wr2".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_dummy_data(&face1, "test/client/**".into());

    // functional check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "test/client/**");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(11, "/**".to_string()));

    // functional check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/**");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/**".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_dummy_data(&face0, 12.into());

    // functional check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(21, "/z1_pub1".to_string()));

    // functional check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z1_pub1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_dummy_data(&face1, 22.into());

    // functional check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), KeyExpr::IdWithSuffix(11, "/z2_pub1".to_string()));

    // functional check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), KeyExpr::IdWithSuffix(31, "/z2_pub1".to_string()));
}

#[test]
fn get_best_key_test() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
        WhatAmI::Client,
        None,
        &config,
    )
    .unwrap();

    let primitives = Arc::new(DummyPrimitives {});
    let face1 = router.new_primitives(primitives.clone());
    let face2 = router.new_primitives(primitives.clone());
    let face3 = router.new_primitives(primitives);

    let root = zread!(router.tables.tables)._get_root().clone();
    let register_expr = |face: &Face, id: ExprId, expr: &str| {
        register_expr(&router.tables, &mut face.state.clone(), id, &expr.into());
    };
    let get_best_key = |resource, suffix, face: &Face| {
        Resource::get_resource(&root, resource)
            .unwrap()
            .get_best_key(suffix, face.state.id)
    };

    register_expr(&face1, 1, "a");
    register_expr(&face2, 2, "a/b");
    register_expr(&face2, 3, "a/b/c");
    register_expr(&face3, 4, "a/d");

    macro_rules! assert_wire_expr {
        ($key:expr, {scope: $scope:expr, suffix: $suffix:expr}) => {
            assert_eq!($key.scope, $scope);
            assert_eq!($key.suffix, $suffix);
        };
    }
    assert_wire_expr!(get_best_key("", "a", &face1), { scope: 1, suffix: "" });
    assert_wire_expr!(get_best_key("", "a/b", &face1), { scope: 1, suffix: "/b" });
    assert_wire_expr!(get_best_key("a", "", &face1), { scope: 1, suffix: "" });
    assert_wire_expr!(get_best_key("a", "/b", &face1), { scope: 1, suffix: "/b" });
    assert_wire_expr!(get_best_key("a/b", "", &face1), { scope: 1, suffix: "/b" });
    assert_wire_expr!(get_best_key("", "e", &face1), { scope: 0, suffix: "e" });
    assert_wire_expr!(get_best_key("", "a", &face2), { scope: 0, suffix: "a" });
    assert_wire_expr!(get_best_key("", "a/b", &face2), { scope: 2, suffix: "" });
    assert_wire_expr!(get_best_key("", "a/b/c", &face2), { scope: 3, suffix: "" });
    assert_wire_expr!(get_best_key("", "a/b/c/d", &face2), { scope: 3, suffix: "/d" });
    assert_wire_expr!(get_best_key("a", "", &face2), { scope: 0, suffix: "a" });
    assert_wire_expr!(get_best_key("a", "/b", &face2), { scope: 2, suffix: "" });
    assert_wire_expr!(get_best_key("a", "/d", &face2), { scope: 0, suffix: "a/d" });
    assert_wire_expr!(get_best_key("a/b", "", &face2), { scope: 2, suffix: "" });
}

#[test]
fn big_key_expr() {
    let config = Config::default();
    let router = Router::new(
        ZenohIdProto::try_from([1]).unwrap(),
        WhatAmI::Client,
        None,
        &config,
    )
    .unwrap();

    let primitives = Arc::new(DummyPrimitives {});
    let face = router.new_primitives(primitives.clone());

    let root = zread!(router.tables.tables)._get_root().clone();
    let key_expr = KeyExpr::new(vec!["a/"; 10000].concat() + "a").unwrap();
    let wire_expr = WireExpr::from(&**key_expr);
    register_expr(&router.tables, &mut face.state.clone(), 1, &wire_expr);
    let res = Resource::get_resource(&root, &key_expr).unwrap();
    root.get_best_key(&key_expr, face.state.id);
    res.get_best_key("/a", face.state.id + 1);
    Resource::get_matches(&face.tables.tables.read().unwrap(), &key_expr);
}
