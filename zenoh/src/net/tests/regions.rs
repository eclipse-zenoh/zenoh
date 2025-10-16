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
    core::WireExpr,
    network::{self, declare::queryable::ext::QueryableInfoType, Request},
    zenoh::{ConsolidationMode, Query, RequestBody},
};
use zenoh_runtime::ZRuntime;

use crate::{
    net::{
        primitives::{DummyPrimitives, EPrimitives},
        routing::{
            dispatcher::gateway::Bound,
            router::{RouterBuilder, DEFAULT_NODE_ID},
        },
        runtime::RuntimeBuilder,
    },
    Config,
};

#[test]
fn test_client_to_client_query_route_computation() {
    const BOUND: Bound = Bound::south(0);
    for mode in [WhatAmI::Client, WhatAmI::Peer, WhatAmI::Router] {
        let mut config = Config::default();
        config.set_mode(Some(mode)).unwrap();

        let mut router = RouterBuilder::new(&config)
            .hat(BOUND, mode)
            .build()
            .unwrap();

        let runtime = ZRuntime::Application
            .block_in_place(RuntimeBuilder::new(config).build())
            .unwrap();
        router.init_hats(runtime).unwrap();

        let src_face = router.new_face(Arc::new(DummyPrimitives), BOUND, WhatAmI::Client, false);

        let dst_buf = Arc::new(RequestBuffer::default());
        let dst_face = router.new_face(dst_buf.clone(), BOUND, WhatAmI::Client, false);

        dst_face.declare_queryable(
            &router.tables,
            0,
            &WireExpr::from("a/b/**"),
            &QueryableInfoType::DEFAULT,
            DEFAULT_NODE_ID,
            &mut |p, m| m.with_mut(|m| p.send_declare(m)),
        );

        src_face.route_query(&mut new_request(1, WireExpr::from("a/b/1")));
        let buf_guard = dst_buf.0.lock().unwrap();
        assert_eq!(&*buf_guard, &[new_request(1, WireExpr::from("a/b/1"))]);
    }
}

#[test]
fn test_north_bound_client_to_south_bound_peer_query_route_computation() {
    let peer = {
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
    let client_face = peer.new_face(client_buf.clone(), Bound::north(), WhatAmI::Client, false);

    let peer_buf = Arc::new(RequestBuffer::default());
    let peer_face = peer.new_face(peer_buf.clone(), Bound::north(), WhatAmI::Peer, false);

    client_face.declare_queryable(
        &peer.tables,
        0,
        &WireExpr::from("a/b/**"),
        &QueryableInfoType::DEFAULT,
        DEFAULT_NODE_ID,
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );

    peer_face.declare_queryable(
        &peer.tables,
        1,
        &WireExpr::from("a/b/**"),
        &QueryableInfoType::DEFAULT,
        DEFAULT_NODE_ID,
        &mut |p, m| m.with_mut(|m| p.send_declare(m)),
    );

    client_face.route_query(&mut new_request(1, WireExpr::from("a/b/1")));

    let client_buf_guard = client_buf.0.lock().unwrap();
    assert_eq!(&*client_buf_guard, &[]);

    let peer_buf_guard = peer_buf.0.lock().unwrap();
    assert_eq!(&*peer_buf_guard, &[]);
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
