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
    common::ZExtBody,
    core::{WireExpr, ZenohIdProto},
    network::{
        self,
        declare::queryable::ext::QueryableInfoType,
        interest::{InterestMode, InterestOptions},
        Declare, Interest, Oam, Request,
    },
    zenoh::{ConsolidationMode, Query, RequestBody},
};
use zenoh_runtime::ZRuntime;

use crate::{
    net::{
        primitives::{DummyPrimitives, EPrimitives, Primitives},
        routing::{
            dispatcher::{face::FaceStateBuilder, gateway::Bound},
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

    const SUBREGION: Bound = Bound::south(0);

    for mode in [WhatAmI::Client, WhatAmI::Peer, WhatAmI::Router] {
        let mut config = Config::default();
        config.set_mode(Some(mode)).unwrap();

        let mut router = RouterBuilder::new(&config)
            .hat(SUBREGION, mode)
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
                Arc::new(DummyPrimitives),
                tables.hats.map(|hat| hat.new_face()),
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
                dst_buf.clone(),
                tables.hats.map(|hat| hat.new_face()),
            )
            .whatami(WhatAmI::Client)
            .build()
        });

        dst_face.declare_queryable(
            &router.tables,
            0,
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
            Bound::north(),
            client_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });

    let peer_buf = Arc::new(RequestBuffer::default());
    let peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Bound::north(),
            client_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
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
    let gateway_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Bound::north(),
            gateway_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let peer_buf = Arc::new(InterestBuffer::default());
    let _peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Bound::north(),
            peer_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    let client_buf = Arc::new(InterestBuffer::default());
    let client_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Bound::session(),
            client_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Client)
        .build()
    });

    {
        let mut tables_guard = router.tables.tables.write().unwrap();
        let tables = &mut *tables_guard;

        tables.hats[Bound::north()]
            .handle_oam(
                &mut tables.data,
                &router.tables,
                &mut Oam {
                    id: network::oam::id::OAM_IS_GATEWAY,
                    body: ZExtBody::Unit,
                    ext_qos: network::oam::ext::QoSType::DEFAULT,
                    ext_tstamp: Some(network::oam::ext::TimestampType::rand()),
                },
                &gateway_face.state.zid,
                gateway_face.state.whatami,
                &mut default_send_declare(),
            )
            .unwrap();
    }

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

    const PEER_SUBREGION: Bound = Bound::south(0);

    let router = {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();

        let mut router = RouterBuilder::new(&config)
            .hat(Bound::north(), WhatAmI::Peer)
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
    let gateway_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            Bound::north(),
            gateway_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
        )
        .whatami(WhatAmI::Peer)
        .build()
    });

    {
        let mut tables_guard = router.tables.tables.write().unwrap();
        let tables = &mut *tables_guard;

        tables.hats[Bound::north()]
            .handle_oam(
                &mut tables.data,
                &router.tables,
                &mut Oam {
                    id: network::oam::id::OAM_IS_GATEWAY,
                    body: ZExtBody::Unit,
                    ext_qos: network::oam::ext::QoSType::DEFAULT,
                    ext_tstamp: Some(network::oam::ext::TimestampType::rand()),
                },
                &gateway_face.state.zid,
                gateway_face.state.whatami,
                &mut default_send_declare(),
            )
            .unwrap();
    }

    let peer_buf = Arc::new(InterestBuffer::default());
    let peer_face = router.new_face(|tables| {
        FaceStateBuilder::new(
            tables.data.new_face_id(),
            ZenohIdProto::rand(),
            PEER_SUBREGION,
            peer_buf.clone(),
            tables.hats.map(|hat| hat.new_face()),
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
