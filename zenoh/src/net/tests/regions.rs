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

#![cfg(feature = "internal_config")]

use std::sync::{Arc, Mutex};

use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::{WireExpr, ZenohIdProto},
    network::{
        self,
        declare::{self, queryable::ext::QueryableInfoType},
        interest::{InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareSubscriber, Interest, Request,
    },
    zenoh::{ConsolidationMode, Query, RequestBody},
};
use zenoh_runtime::ZRuntime;
use zenoh_transport::Bound;

use crate::{
    net::{
        primitives::{DummyPrimitives, EPrimitives, Primitives},
        routing::{
            dispatcher::{face::FaceStateBuilder, region::Region},
            router::{RouterBuilder, DEFAULT_NODE_ID},
            RoutingContext,
        },
        runtime::RuntimeBuilder,
    },
    Config,
};

#[test]
fn test_client_to_client_query_route_computation() {
    zenoh_util::try_init_log_from_env();

    const SUBREGION: Region = Region::Subregion {
        id: 0,
        mode: WhatAmI::Client,
    };

    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();

    let mut router = RouterBuilder::new(&config)
        .hat(SUBREGION, WhatAmI::Client)
        .build()
        .unwrap();

    let runtime = ZRuntime::Application
        .block_in_place(RuntimeBuilder::new(config).build())
        .unwrap();
    router.init_hats(runtime).unwrap();

    let src_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            SUBREGION,
            Bound::North,
            Arc::new(DummyPrimitives),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });
    let dst_buf = Arc::new(RequestBuffer::default());
    let dst_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            SUBREGION,
            Bound::North,
            dst_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });

    dst_face.declare_queryable(
        &router.tables,
        1,
        &WireExpr::from("a/b/**"),
        &QueryableInfoType::DEFAULT,
        DEFAULT_NODE_ID,
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );

    let mut msg = new_request(1, WireExpr::from("a/b/1"));

    src_face.route_query(&mut msg);
    let buf_guard = dst_buf.0.lock().unwrap();
    assert_eq!(&*buf_guard, &[msg]);
}

#[test]
fn test_north_bound_client_to_south_bound_peer_query_route_computation() {
    zenoh_util::try_init_log_from_env();

    let router = {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();

        let mut router = RouterBuilder::new(&config).build().unwrap();

        let runtime = ZRuntime::Application
            .block_in_place(RuntimeBuilder::new(config).build())
            .unwrap();
        router.init_hats(runtime).unwrap();
        router
    };

    let client_buf = Arc::new(RequestBuffer::default());
    let client_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::South,
            client_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });

    let peer_buf = Arc::new(RequestBuffer::default());
    let peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::North,
            client_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    client_face.declare_queryable(
        &router.tables,
        0,
        &WireExpr::from("a/b/**"),
        &QueryableInfoType::DEFAULT,
        DEFAULT_NODE_ID,
        &mut default_send_declare(),
    );

    peer_face.declare_queryable(
        &router.tables,
        1,
        &WireExpr::from("a/b/**"),
        &QueryableInfoType::DEFAULT,
        DEFAULT_NODE_ID,
        &mut default_send_declare(),
    );

    client_face.route_query(&mut new_request(1, WireExpr::from("a/b/1")));

    let client_buf_guard = client_buf.0.lock().unwrap();
    assert_eq!(&*client_buf_guard, &[]);

    let peer_buf_guard = peer_buf.0.lock().unwrap();
    assert_eq!(&*peer_buf_guard, &[]);
}

#[test]
fn test_peer_interest_propagation() {
    zenoh_util::try_init_log_from_env();

    let router = {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();

        let mut router = RouterBuilder::new(&config).build().unwrap();

        let runtime = ZRuntime::Application
            .block_in_place(RuntimeBuilder::new(config).build())
            .unwrap();
        router.init_hats(runtime).unwrap();
        router
    };

    let gateway_buf = Arc::new(InterestBuffer::default());
    let _gateway_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::South,
            gateway_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let peer_buf = Arc::new(InterestBuffer::default());
    let _peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::North,
            peer_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let client_buf = Arc::new(InterestBuffer::default());
    let client_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::Local,
            Bound::North,
            client_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });

    let mut msg = Interest {
        id: 1,
        mode: InterestMode::CurrentFuture,
        options: InterestOptions::ALL,
        wire_expr: None,
        ext_qos: network::interest::ext::QoSType::INTEREST,
        ext_tstamp: None,
        ext_nodeid: network::interest::ext::NodeIdType::DEFAULT,
    };

    client_face.send_interest(&mut msg);

    let client_buf_guard = client_buf.0.lock().unwrap();
    assert_eq!(&*client_buf_guard, &[]);

    let peer_buf_guard = peer_buf.0.lock().unwrap();
    assert_eq!(&*peer_buf_guard, &[]);

    let gateway_buf_guard = gateway_buf.0.lock().unwrap();
    assert_eq!(&*gateway_buf_guard, &[msg]);
}

#[test]
fn test_peer_gateway_interest_propagation() {
    zenoh_util::try_init_log_from_env();

    const PEER_SUBREGION: Region = Region::Subregion {
        id: 0,
        mode: WhatAmI::Peer,
    };

    let router = {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();

        let mut router = RouterBuilder::new(&config)
            .hat(Region::North, WhatAmI::Peer)
            .hat(PEER_SUBREGION, WhatAmI::Peer)
            .build()
            .unwrap();

        let runtime = ZRuntime::Application
            .block_in_place(RuntimeBuilder::new(config).build())
            .unwrap();
        router.init_hats(runtime).unwrap();
        router
    };

    let gateway_buf = Arc::new(InterestBuffer::default());
    let _gateway_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::South,
            gateway_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let peer_buf = Arc::new(InterestBuffer::default());
    let peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            PEER_SUBREGION,
            Bound::North,
            peer_buf.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let mut msg = Interest {
        id: 1,
        mode: InterestMode::CurrentFuture,
        options: InterestOptions::ALL,
        wire_expr: None,
        ext_qos: network::interest::ext::QoSType::INTEREST,
        ext_tstamp: None,
        ext_nodeid: network::interest::ext::NodeIdType::DEFAULT,
    };

    peer_face.send_interest(&mut msg);

    let peer_buf_guard = peer_buf.0.lock().unwrap();
    assert_eq!(&*peer_buf_guard, &[]);

    let gateway_buf_guard = gateway_buf.0.lock().unwrap();
    assert_eq!(&*gateway_buf_guard, &[msg]);
}

fn test_declaration_propagation_to_late_faces(mode0: WhatAmI, mode1: WhatAmI, mode2: WhatAmI) {
    zenoh_util::try_init_log_from_env();

    let router = {
        let config = Config::default();

        let mut router = RouterBuilder::new(&config)
            .hat(Region::North, mode1)
            .hat(Region::Subregion { id: 0, mode: mode2 }, mode2)
            .build()
            .unwrap();

        let runtime = ZRuntime::Application
            .block_in_place(RuntimeBuilder::new(config).build())
            .unwrap();
        router.init_hats(runtime).unwrap();
        router
    };

    // 1. Client north hat receives declarations from some client face
    // 2. New router face joins
    // 3. Did the router receive said declaration?

    // TODO: right now there are two limitations to this test:
    // 1. We only handle a `R/? - C/P - P` scenario
    // 2. The declaration is a subscriber only

    let buf2 = Arc::new(DeclarationBuffer::default());
    let face2 = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::Subregion { id: 0, mode: mode2 },
            Bound::North,
            buf2.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(mode2)
        .build()
    });

    let msg = Declare {
        interest_id: None,
        ext_qos: declare::ext::QoSType::INTEREST,
        ext_tstamp: None,
        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
            id: 1,
            wire_expr: WireExpr::from("**/a/b"),
        }),
    };

    face2.send_declare(&mut msg.clone());

    let buf0 = Arc::new(DeclarationBuffer::default());
    let _face0 = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Region::North,
            Bound::North,
            buf0.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .whatami(mode0)
        .build()
    });

    let buf0_guard = buf2.0.lock().unwrap();
    assert_eq!(&*buf0_guard, &[]);

    let buf1_guard = buf0.0.lock().unwrap();
    assert_eq!(&*buf1_guard, &[msg]);
}

#[test]
fn test_declaration_propagation_to_late_faces_router_client_peer() {
    test_declaration_propagation_to_late_faces(WhatAmI::Router, WhatAmI::Client, WhatAmI::Peer);
}

#[test]
fn test_declaration_propagation_to_late_faces_client_peer_client() {
    test_declaration_propagation_to_late_faces(WhatAmI::Client, WhatAmI::Peer, WhatAmI::Client);
}

// FIXME(regions): this fails because face0 is considered local
#[ignore]
fn _test_declaration_propagation_to_late_faces_router_router_peer() {
    test_declaration_propagation_to_late_faces(WhatAmI::Router, WhatAmI::Router, WhatAmI::Peer);
}

#[test]
fn test_declaration_propagation_to_late_faces_router_peer_client() {
    test_declaration_propagation_to_late_faces(WhatAmI::Router, WhatAmI::Peer, WhatAmI::Client);
}

#[derive(Debug, Default)]
/// [`EPrimitives`] impl that only stores [`Request`]s.
struct RequestBuffer(Mutex<Vec<Request>>);

impl EPrimitives for RequestBuffer {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn send_interest(
        &self,
        _ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Interest>,
    ) {
    }

    fn send_declare(
        &self,
        _ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Declare>,
    ) {
    }

    fn send_push(
        &self,
        _msg: &mut zenoh_protocol::network::Push,
        _reliability: zenoh_protocol::core::Reliability,
    ) {
    }

    fn send_request(&self, msg: &mut Request) {
        let mut buf = self.0.lock().unwrap();
        buf.push(msg.clone());
    }

    fn send_response(&self, _msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, _msg: &mut zenoh_protocol::network::ResponseFinal) {}
}

#[derive(Debug, Default)]
/// [`EPrimitives`] impl that only stores [`Interest`]s.
struct InterestBuffer(Mutex<Vec<Interest>>);

impl EPrimitives for InterestBuffer {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn send_interest(
        &self,
        ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Interest>,
    ) {
        let mut buf = self.0.lock().unwrap();
        buf.push(ctx.msg.clone());
    }

    fn send_declare(
        &self,
        _ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Declare>,
    ) {
    }

    fn send_push(
        &self,
        _msg: &mut zenoh_protocol::network::Push,
        _reliability: zenoh_protocol::core::Reliability,
    ) {
    }

    fn send_request(&self, _msg: &mut Request) {}

    fn send_response(&self, _msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, _msg: &mut zenoh_protocol::network::ResponseFinal) {}
}

#[derive(Debug, Default)]
/// [`EPrimitives`] impl that only stores [`Interest`]s.
struct DeclarationBuffer(Mutex<Vec<Declare>>);

impl EPrimitives for DeclarationBuffer {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn send_interest(
        &self,
        _ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Interest>,
    ) {
    }

    fn send_declare(
        &self,
        ctx: crate::net::routing::RoutingContext<&mut zenoh_protocol::network::Declare>,
    ) {
        let mut buf = self.0.lock().unwrap();
        buf.push(ctx.msg.clone());
    }

    fn send_push(
        &self,
        _msg: &mut zenoh_protocol::network::Push,
        _reliability: zenoh_protocol::core::Reliability,
    ) {
    }

    fn send_request(&self, _msg: &mut Request) {}

    fn send_response(&self, _msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, _msg: &mut zenoh_protocol::network::ResponseFinal) {}
}

fn new_request(id: u32, wire_expr: WireExpr<'static>) -> Request {
    Request {
        id,
        wire_expr,
        ext_qos: network::request::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: network::request::ext::NodeIdType::DEFAULT,
        ext_target: network::request::ext::QueryTarget::DEFAULT,
        ext_budget: None,
        ext_timeout: None,
        payload: RequestBody::Query(Query {
            consolidation: ConsolidationMode::Auto,
            parameters: String::default(),
            ext_sinfo: None,
            ext_body: None,
            ext_attachment: None,
            ext_unknown: Vec::default(),
        }),
    }
}

fn default_send_declare() -> impl FnMut(&Arc<dyn EPrimitives + Send + Sync>, RoutingContext<Declare>)
{
    |p, m| m.with_mut(|m| p.send_declare(m))
}
