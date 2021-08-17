//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::Arc;
use std::convert::TryInto;
use uhlc::HLC;
use zenoh::net::protocol::core::rname::intersect;
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, QueryConsolidation, QueryTarget, Reliability,
    ResKey, SubInfo, SubMode, ZInt,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::proto::{DataInfo, RoutingContext};
use zenoh::net::routing::router::*;
use zenoh::net::transport::{DummyPrimitives, Primitives};
use zenoh_util::zlock;

#[test]
fn base_test() {
    let mut tables = Tables::new(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        Some(Arc::new(HLC::default())),
    );
    let primitives = Arc::new(DummyPrimitives::new());
    let face = tables.open_face(PeerId::new(0, [0; 16]), whatami::CLIENT, primitives);
    declare_resource(
        &mut tables,
        &mut face.upgrade().unwrap(),
        1,
        0,
        "/one/two/three",
    );
    declare_resource(
        &mut tables,
        &mut face.upgrade().unwrap(),
        2,
        0,
        "/one/deux/trois",
    );

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    declare_client_subscription(
        &mut tables,
        &mut face.upgrade().unwrap(),
        1,
        "/four/five",
        &sub_info,
    );

    Tables::print(&tables);
}

#[test]
fn match_test() {
    let rnames = [
        "/",
        "/a",
        "/a/",
        "/a/b",
        "/*",
        "/abc",
        "/abc/",
        "/*/",
        "xxx",
        "/ab*",
        "/abcd",
        "/ab*d",
        "/ab",
        "/ab/*",
        "/a/*/c/*/e",
        "/a/b/c/d/e",
        "/a/*b/c/*d/e",
        "/a/xb/c/xd/e",
        "/a/c/e",
        "/a/b/c/d/x/e",
        "/ab*cd",
        "/abxxcxxd",
        "/abxxcxxcd",
        "/abxxcxxcdx",
        "/**",
        "/a/b/c",
        "/a/b/c/",
        "/**/",
        "/ab/**",
        "/**/xyz",
        "/a/b/xyz/d/e/f/xyz",
        "/**/xyz*xyz",
        "/a/b/xyz/d/e/f/xyz",
        "/a/**/c/**/e",
        "/a/b/b/b/c/d/d/d/e",
        "/a/**/c/*/e/*",
        "/a/b/b/b/c/d/d/c/d/e/f",
        "/a/**/c/*/e/*",
        "/x/abc",
        "/x/*",
        "/x/abc*",
        "/x/*abc",
        "/x/a*",
        "/x/a*de",
        "/x/abc*de",
        "/x/a*d*e",
        "/x/a*e",
        "/x/a*c*e",
        "/x/ade",
        "/x/c*",
        "/x/*d",
        "/x/*e",
    ];

    let mut tables = Tables::new(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        Some(Arc::new(HLC::default())),
    );
    let primitives = Arc::new(DummyPrimitives::new());
    let face = tables.open_face(PeerId::new(0, [0; 16]), whatami::CLIENT, primitives);
    for (i, rname) in rnames.iter().enumerate() {
        declare_resource(
            &mut tables,
            &mut face.upgrade().unwrap(),
            i.try_into().unwrap(),
            0,
            rname,
        );
    }

    for rname1 in rnames.iter() {
        let res_matches = Resource::get_matches(&tables, rname1);
        for rname2 in rnames.iter() {
            if res_matches
                .iter()
                .map(|m| m.upgrade().unwrap().name())
                .any(|x| x == **rname2)
            {
                assert!(intersect(rname1, rname2));
            } else {
                assert!(!intersect(rname1, rname2));
            }
        }
    }
}

#[test]
fn clean_test() {
    let mut tables = Tables::new(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        Some(Arc::new(HLC::default())),
    );

    let primitives = Arc::new(DummyPrimitives::new());
    let face0 = tables.open_face(PeerId::new(0, [0; 16]), whatami::CLIENT, primitives);
    assert!(face0.upgrade().is_some());

    // --------------
    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 1, 0, "/todrop1");
    let optres1 =
        Resource::get_resource(tables._get_root(), "/todrop1").map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    declare_resource(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        2,
        0,
        "/todrop1/todrop11",
    );
    let optres2 = Resource::get_resource(tables._get_root(), "/todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 3, 0, "/**");
    let optres3 = Resource::get_resource(tables._get_root(), "/**").map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    assert!(res3.upgrade().is_some());

    undeclare_resource(&mut tables, &mut face0.upgrade().unwrap(), 1);
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    undeclare_resource(&mut tables, &mut face0.upgrade().unwrap(), 2);
    assert!(!res1.upgrade().is_some());
    assert!(!res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    undeclare_resource(&mut tables, &mut face0.upgrade().unwrap(), 3);
    assert!(!res1.upgrade().is_some());
    assert!(!res2.upgrade().is_some());
    assert!(!res3.upgrade().is_some());

    // --------------
    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 1, 0, "/todrop1");
    let optres1 =
        Resource::get_resource(tables._get_root(), "/todrop1").map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        0,
        "/todrop1/todrop11",
        &sub_info,
    );
    let optres2 = Resource::get_resource(tables._get_root(), "/todrop1/todrop11")
        .map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    assert!(res2.upgrade().is_some());

    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        1,
        "/todrop12",
        &sub_info,
    );
    let optres3 = Resource::get_resource(tables._get_root(), "/todrop1/todrop12")
        .map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();
    assert!(res3.upgrade().is_some());

    forget_client_subscription(&mut tables, &mut face0.upgrade().unwrap(), 1, "/todrop12");
    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(!res3.upgrade().is_some());

    forget_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        0,
        "/todrop1/todrop11",
    );
    assert!(res1.upgrade().is_some());
    assert!(!res2.upgrade().is_some());
    assert!(!res3.upgrade().is_some());

    undeclare_resource(&mut tables, &mut face0.upgrade().unwrap(), 1);
    assert!(!res1.upgrade().is_some());
    assert!(!res2.upgrade().is_some());
    assert!(!res3.upgrade().is_some());

    // --------------
    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 2, 0, "/todrop3");
    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        0,
        "/todrop3",
        &sub_info,
    );
    let optres1 =
        Resource::get_resource(tables._get_root(), "/todrop3").map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    assert!(res1.upgrade().is_some());

    forget_client_subscription(&mut tables, &mut face0.upgrade().unwrap(), 0, "/todrop3");
    assert!(res1.upgrade().is_some());

    undeclare_resource(&mut tables, &mut face0.upgrade().unwrap(), 2);
    assert!(!res1.upgrade().is_some());

    // --------------
    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 3, 0, "/todrop4");
    declare_resource(&mut tables, &mut face0.upgrade().unwrap(), 4, 0, "/todrop5");
    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        0,
        "/todrop5",
        &sub_info,
    );
    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        0,
        "/todrop6",
        &sub_info,
    );

    let optres1 =
        Resource::get_resource(tables._get_root(), "/todrop4").map(|res| Arc::downgrade(&res));
    assert!(optres1.is_some());
    let res1 = optres1.unwrap();
    let optres2 =
        Resource::get_resource(tables._get_root(), "/todrop5").map(|res| Arc::downgrade(&res));
    assert!(optres2.is_some());
    let res2 = optres2.unwrap();
    let optres3 =
        Resource::get_resource(tables._get_root(), "/todrop6").map(|res| Arc::downgrade(&res));
    assert!(optres3.is_some());
    let res3 = optres3.unwrap();

    assert!(res1.upgrade().is_some());
    assert!(res2.upgrade().is_some());
    assert!(res3.upgrade().is_some());

    tables.close_face(&face0);
    assert!(!face0.upgrade().is_some());
    assert!(!res1.upgrade().is_some());
    assert!(!res2.upgrade().is_some());
    assert!(!res3.upgrade().is_some());
}

pub struct ClientPrimitives {
    data: std::sync::Mutex<Option<ResKey>>,
    mapping: std::sync::Mutex<std::collections::HashMap<ZInt, String>>,
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
    fn get_name(&self, reskey: &ResKey) -> String {
        let mapping = self.mapping.lock().unwrap();
        match reskey {
            ResKey::RName(name) => name.clone(),
            ResKey::RId(id) => mapping.get(id).unwrap().clone(),
            ResKey::RIdWithSuffix(id, suffix) => {
                [&mapping.get(id).unwrap()[..], &suffix[..]].concat()
            }
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
    fn get_last_key(&self) -> Option<ResKey> {
        self.data.lock().unwrap().as_ref().cloned()
    }
}

impl Primitives for ClientPrimitives {
    fn decl_resource(&self, rid: ZInt, reskey: &ResKey) {
        let name = self.get_name(reskey);
        zlock!(self.mapping).insert(rid, name);
    }

    fn forget_resource(&self, rid: ZInt) {
        zlock!(self.mapping).remove(&rid);
    }

    fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_subscriber(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(
        &self,
        _reskey: &ResKey,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn send_data(
        &self,
        reskey: &ResKey,
        _payload: ZBuf,
        _channel: Channel,
        _congestion_control: CongestionControl,
        _info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
        *zlock!(self.data) = Some(reskey.clone());
    }

    fn send_query(
        &self,
        _reskey: &ResKey,
        _predicate: &str,
        _qid: ZInt,
        _target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
    }

    fn send_reply_data(
        &self,
        _qid: ZInt,
        _replier_kind: ZInt,
        _replier_id: PeerId,
        _reskey: ResKey,
        _info: Option<DataInfo>,
        _payload: ZBuf,
    ) {
    }
    fn send_reply_final(&self, _qid: ZInt) {}

    fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
    }

    fn send_close(&self) {}
}

#[test]
fn client_test() {
    let mut tables = Tables::new(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        Some(Arc::new(HLC::default())),
    );
    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    let primitives0 = Arc::new(ClientPrimitives::new());
    let face0 = tables.open_face(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        primitives0.clone(),
    );
    declare_resource(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        11,
        0,
        "/test/client",
    );
    primitives0.decl_resource(11, &ResKey::RName("/test/client".to_string()));
    declare_client_subscription(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        11,
        "/**",
        &sub_info,
    );
    declare_resource(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        12,
        11,
        "/z1_pub1",
    );
    primitives0.decl_resource(12, &ResKey::RIdWithSuffix(11, "/z1_pub1".to_string()));

    let primitives1 = Arc::new(ClientPrimitives::new());
    let face1 = tables.open_face(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        primitives1.clone(),
    );
    declare_resource(
        &mut tables,
        &mut face1.upgrade().unwrap(),
        21,
        0,
        "/test/client",
    );
    primitives1.decl_resource(21, &ResKey::RName("/test/client".to_string()));
    declare_client_subscription(
        &mut tables,
        &mut face1.upgrade().unwrap(),
        21,
        "/**",
        &sub_info,
    );
    declare_resource(
        &mut tables,
        &mut face1.upgrade().unwrap(),
        22,
        21,
        "/z2_pub1",
    );
    primitives1.decl_resource(22, &ResKey::RIdWithSuffix(21, "/z2_pub1".to_string()));

    let primitives2 = Arc::new(ClientPrimitives::new());
    let face2 = tables.open_face(
        PeerId::new(0, [0; 16]),
        whatami::CLIENT,
        primitives2.clone(),
    );
    declare_resource(
        &mut tables,
        &mut face2.upgrade().unwrap(),
        31,
        0,
        "/test/client",
    );
    primitives2.decl_resource(31, &ResKey::RName("/test/client".to_string()));
    declare_client_subscription(
        &mut tables,
        &mut face2.upgrade().unwrap(),
        31,
        "/**",
        &sub_info,
    );

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_data(
        &tables,
        &face0.upgrade().unwrap(),
        0,
        "/test/client/z1_wr1",
        Channel::default(),
        CongestionControl::default(),
        None,
        ZBuf::new(),
        None,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "/test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), ResKey::RIdWithSuffix(21, "/z1_wr1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "/test/client/z1_wr1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), ResKey::RIdWithSuffix(31, "/z1_wr1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_data(
        &tables,
        &face0.upgrade().unwrap(),
        11,
        "/z1_wr2",
        Channel::default(),
        CongestionControl::default(),
        None,
        ZBuf::new(),
        None,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "/test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), ResKey::RIdWithSuffix(21, "/z1_wr2".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "/test/client/z1_wr2");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), ResKey::RIdWithSuffix(31, "/z1_wr2".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_data(
        &tables,
        &face1.upgrade().unwrap(),
        0,
        "/test/client/**",
        Channel::default(),
        CongestionControl::default(),
        None,
        ZBuf::new(),
        None,
    );

    // functionnal check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "/test/client/**");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), ResKey::RIdWithSuffix(11, "/**".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "/test/client/**");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), ResKey::RIdWithSuffix(31, "/**".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_data(
        &tables,
        &face0.upgrade().unwrap(),
        12,
        "",
        Channel::default(),
        CongestionControl::default(),
        None,
        ZBuf::new(),
        None,
    );

    // functionnal check
    assert!(primitives1.get_last_name().is_some());
    assert_eq!(primitives1.get_last_name().unwrap(), "/test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), ResKey::RIdWithSuffix(21, "/z1_pub1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "/test/client/z1_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), ResKey::RIdWithSuffix(31, "/z1_pub1".to_string()));

    primitives0.clear_data();
    primitives1.clear_data();
    primitives2.clear_data();
    route_data(
        &tables,
        &face1.upgrade().unwrap(),
        22,
        "",
        Channel::default(),
        CongestionControl::default(),
        None,
        ZBuf::new(),
        None,
    );

    // functionnal check
    assert!(primitives0.get_last_name().is_some());
    assert_eq!(primitives0.get_last_name().unwrap(), "/test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives1.get_last_key().unwrap(), ResKey::RIdWithSuffix(11, "/z2_pub1".to_string()));

    // functionnal check
    assert!(primitives2.get_last_name().is_some());
    assert_eq!(primitives2.get_last_name().unwrap(), "/test/client/z2_pub1");
    // mapping strategy check
    // assert_eq!(primitives2.get_last_key().unwrap(), ResKey::RIdWithSuffix(31, "/z2_pub1".to_string()));
}
