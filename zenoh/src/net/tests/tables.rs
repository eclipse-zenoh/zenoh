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
use crate::net::routing::router::{self, *};
use std::convert::{TryFrom, TryInto};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use uhlc::HLC;
use zenoh_buffers::ZBuf;
use zenoh_config::defaults::queries_default_timeout;
use zenoh_core::zlock;
use zenoh_protocol::core::Encoding;
use zenoh_protocol::core::{
    key_expr::keyexpr, ExprId, Reliability, WhatAmI, WireExpr, ZenohId, EMPTY_EXPR_ID,
};
use zenoh_protocol::network::declare::subscriber::ext::SubscriberInfo;
use zenoh_protocol::network::declare::Mode;
use zenoh_protocol::network::{ext, Declare, DeclareBody, DeclareKeyExpr};
use zenoh_protocol::zenoh::{PushBody, Put};
use zenoh_transport::primitives::{DummyPrimitives, Primitives};

#[test]
fn base_test() {
    let tables = TablesLock {
        tables: RwLock::new(Tables::new(
            ZenohId::try_from([1]).unwrap(),
            WhatAmI::Client,
            Some(Arc::new(HLC::default())),
            false,
            true,
            Duration::from_millis(queries_default_timeout),
        )),
        ctrl_lock: Mutex::new(()),
        queries_lock: RwLock::new(()),
    };

    let primitives = Arc::new(DummyPrimitives::new());
    let face = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives,
    );
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
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("four/five"),
        &sub_info,
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

    let tables = TablesLock {
        tables: RwLock::new(Tables::new(
            ZenohId::try_from([1]).unwrap(),
            WhatAmI::Client,
            Some(Arc::new(HLC::default())),
            false,
            true,
            Duration::from_millis(queries_default_timeout),
        )),
        ctrl_lock: Mutex::new(()),
        queries_lock: RwLock::new(()),
    };
    let primitives = Arc::new(DummyPrimitives::new());
    let face = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives,
    );
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

#[test]
fn clean_test() {
    let tables = TablesLock {
        tables: RwLock::new(Tables::new(
            ZenohId::try_from([1]).unwrap(),
            WhatAmI::Client,
            Some(Arc::new(HLC::default())),
            false,
            true,
            Duration::from_millis(queries_default_timeout),
        )),
        ctrl_lock: Mutex::new(()),
        queries_lock: RwLock::new(()),
    };

    let primitives = Arc::new(DummyPrimitives::new());
    let face0 = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives,
    );
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

    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop1/todrop11".into(),
        &sub_info,
    );
    let optres2 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("/todrop12"),
        &sub_info,
    );
    let optres3 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop1/todrop12")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    assert!(res3.upgrade().is_some());

    forget_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(1).with_suffix("/todrop12"),
    );
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_none());

    forget_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop1/todrop11".into(),
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
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop3".into(),
        &sub_info,
    );
    let optres1 = Resource::get_resource(zread!(tables.tables)._get_root(), "todrop3")
        .map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    forget_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop3".into(),
    );
    assert!(res1.upgrade().is_some());

    unregister_expr(&tables, &mut face0.upgrade().unwrap(), 2);
    assert!(res1.upgrade().is_none());

    // --------------
    register_expr(&tables, &mut face0.upgrade().unwrap(), 3, &"todrop4".into());
    register_expr(&tables, &mut face0.upgrade().unwrap(), 4, &"todrop5".into());
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop5".into(),
        &sub_info,
    );
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &"todrop6".into(),
        &sub_info,
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

    router::close_face(&tables, &face0);
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

#[test]
fn client_test() {
    let tables = TablesLock {
        tables: RwLock::new(Tables::new(
            ZenohId::try_from([1]).unwrap(),
            WhatAmI::Client,
            Some(Arc::new(HLC::default())),
            false,
            true,
            Duration::from_millis(queries_default_timeout),
        )),
        ctrl_lock: Mutex::new(()),
        queries_lock: RwLock::new(()),
    };

    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable,
        mode: Mode::Push,
    };

    let primitives0 = Arc::new(ClientPrimitives::new());

    let face0 = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives0.clone(),
    );
    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        11,
        &"test/client".into(),
    );
    primitives0.send_declare(Declare {
        ext_qos: ext::QoSType::declare_default(),
        ext_tstamp: None,
        ext_nodeid: ext::NodeIdType::default(),
        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
            id: 11,
            wire_expr: "test/client".into(),
        }),
    });
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face0.upgrade().unwrap(),
        &WireExpr::from(11).with_suffix("/**"),
        &sub_info,
    );
    register_expr(
        &tables,
        &mut face0.upgrade().unwrap(),
        12,
        &WireExpr::from(11).with_suffix("/z1_pub1"),
    );
    primitives0.send_declare(Declare {
        ext_qos: ext::QoSType::declare_default(),
        ext_tstamp: None,
        ext_nodeid: ext::NodeIdType::default(),
        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
            id: 12,
            wire_expr: WireExpr::from(11).with_suffix("/z1_pub1"),
        }),
    });

    let primitives1 = Arc::new(ClientPrimitives::new());
    let face1 = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives1.clone(),
    );
    register_expr(
        &tables,
        &mut face1.upgrade().unwrap(),
        21,
        &"test/client".into(),
    );
    primitives1.send_declare(Declare {
        ext_qos: ext::QoSType::declare_default(),
        ext_tstamp: None,
        ext_nodeid: ext::NodeIdType::default(),
        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
            id: 21,
            wire_expr: "test/client".into(),
        }),
    });
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face1.upgrade().unwrap(),
        &WireExpr::from(21).with_suffix("/**"),
        &sub_info,
    );
    register_expr(
        &tables,
        &mut face1.upgrade().unwrap(),
        22,
        &WireExpr::from(21).with_suffix("/z2_pub1"),
    );
    primitives1.send_declare(Declare {
        ext_qos: ext::QoSType::declare_default(),
        ext_tstamp: None,
        ext_nodeid: ext::NodeIdType::default(),
        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
            id: 22,
            wire_expr: WireExpr::from(21).with_suffix("/z2_pub1"),
        }),
    });

    let primitives2 = Arc::new(ClientPrimitives::new());
    let face2 = zwrite!(tables.tables).open_face(
        ZenohId::try_from([1]).unwrap(),
        WhatAmI::Client,
        primitives2.clone(),
    );
    register_expr(
        &tables,
        &mut face2.upgrade().unwrap(),
        31,
        &"test/client".into(),
    );
    primitives2.send_declare(Declare {
        ext_qos: ext::QoSType::declare_default(),
        ext_tstamp: None,
        ext_nodeid: ext::NodeIdType::default(),
        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
            id: 31,
            wire_expr: "test/client".into(),
        }),
    });
    declare_client_subscription(
        &tables,
        zread!(tables.tables),
        &mut face2.upgrade().unwrap(),
        &WireExpr::from(31).with_suffix("/**"),
        &sub_info,
    );

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();

    full_reentrant_route_data(
        &tables.tables,
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
        &tables.tables,
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
        &tables.tables,
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
        &tables.tables,
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
        &tables.tables,
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
