//
// Copyright (c) 2026 ZettaScale Technology
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

#![allow(dead_code)]

//! Routing test framework.
//!
//! Provides building blocks for writing routing integration tests that interact directly with a
//! [`Gateway`] without the transport and orchestrator layers. Tests can configure one or more
//! gateways, attach mock faces representing arbitrary remote nodes (clients, peers, routers),
//! inject messages into those faces, and observe every message that the gateway routes to any other
//! face.

mod adminspace;
mod declare;
mod forwarding;
mod interest;

use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Mutex},
};

use futures::executor::block_on;
use tracing_subscriber::EnvFilter;
use zenoh_config::{Config, ZenohId};
use zenoh_protocol::{
    core::{Bound, ExprId, Region, Reliability, WhatAmI, WireExpr, ZenohIdProto},
    network::{
        declare::{
            common::ext::WireExprType,
            queryable::{QueryableId, UndeclareQueryable},
            subscriber::{SubscriberId, UndeclareSubscriber},
            token::{TokenId, UndeclareToken},
            DeclareQueryable, DeclareSubscriber, DeclareToken,
        },
        ext::{self, NodeIdType},
        interest::{InterestId, InterestMode, InterestOptions},
        request::ext::QueryTarget,
        Declare, DeclareBody, DeclareFinal, DeclareKeyExpr, Interest, NetworkBody, NetworkBodyMut,
        NetworkMessageMut, Oam, Push, Request, RequestId, Response, ResponseFinal,
    },
    zenoh::{PushBody, Put, Query, RequestBody},
};
use zenoh_transport::{
    unicast::test_helpers::MockTransportUnicastInner, TransportPeerEventHandler,
};

use crate::net::{
    primitives::{DeMux, EPrimitives, Primitives},
    routing::{
        dispatcher::face::Face,
        gateway::{Gateway, GatewayBuilder},
        RoutingContext,
    },
    runtime::{Runtime, RuntimeBuilder},
};

pub(crate) fn try_init_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}

/// Every message type that [`RecordingPrimitives`] can receive, stored verbatim.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Push(Push),
    Declare(Declare),
    Request(Request),
    Response(Response),
    ResponseFinal(ResponseFinal),
    Interest(Interest),
    Oam(Oam),
}

/// An [`EPrimitives`] + [`Primitives`] implementation that records every message delivered to it.
///
/// All messages are stored as [`Message`] variants in arrival order.
pub(crate) struct RecordingPrimitives {
    messages: Mutex<Vec<Message>>,
}

impl RecordingPrimitives {
    pub(crate) fn new() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn with_messages<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&[Message]) -> T,
    {
        let msgs = self.messages.lock().unwrap();
        f(&msgs)
    }

    pub(crate) fn pushes(&self) -> Vec<Push> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Push(p) = m {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn subscribers(&self) -> Vec<DeclareSubscriber> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::DeclareSubscriber(d),
                    ..
                }) = m
                {
                    Some(d.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn undeclared_subscribers(&self) -> Vec<UndeclareSubscriber> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::UndeclareSubscriber(u),
                    ..
                }) = m
                {
                    Some(u.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn queryables(&self) -> Vec<DeclareQueryable> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::DeclareQueryable(d),
                    ..
                }) = m
                {
                    Some(d.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn undeclared_queryables(&self) -> Vec<UndeclareQueryable> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::UndeclareQueryable(u),
                    ..
                }) = m
                {
                    Some(u.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn tokens(&self) -> Vec<DeclareToken> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::DeclareToken(d),
                    ..
                }) = m
                {
                    Some(d.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn undeclared_tokens(&self) -> Vec<UndeclareToken> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::UndeclareToken(u),
                    ..
                }) = m
                {
                    Some(u.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Request(r) = m {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn responses(&self) -> Vec<Response> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Response(r) = m {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn response_finals(&self) -> Vec<ResponseFinal> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::ResponseFinal(r) = m {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn interests(&self) -> Vec<Interest> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Interest(i) = m {
                    Some(i.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn declare_finals(&self) -> Vec<DeclareFinal> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Declare(Declare {
                    body: DeclareBody::DeclareFinal(u),
                    ..
                }) = m
                {
                    Some(u.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn oams(&self) -> Vec<Oam> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| {
                if let Message::Oam(o) = m {
                    Some(o.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Discard all recorded messages.
    pub(crate) fn clear(&self) {
        self.messages.lock().unwrap().clear();
    }
}

impl Primitives for RecordingPrimitives {
    fn send_interest(&self, msg: &mut Interest) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::Interest(msg.clone()));
    }

    fn send_declare(&self, msg: &mut Declare) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::Declare(msg.clone()));
    }

    fn send_push_consume(&self, msg: &mut Push, _reliability: Reliability, _consume: bool) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::Push(msg.clone()));
    }

    fn send_request(&self, msg: &mut Request) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::Request(msg.clone()));
    }

    fn send_response(&self, msg: &mut Response) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::Response(msg.clone()));
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        self.messages
            .lock()
            .unwrap()
            .push(Message::ResponseFinal(msg.clone()));
    }

    fn send_close(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl EPrimitives for RecordingPrimitives {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) -> bool {
        Primitives::send_interest(self, ctx.msg);
        false
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) -> bool {
        Primitives::send_declare(self, ctx.msg);
        false
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) -> bool {
        self.send_push_consume(msg, reliability, true);
        false
    }

    fn send_request(&self, msg: &mut Request) -> bool {
        Primitives::send_request(self, msg);
        false
    }

    fn send_response(&self, msg: &mut Response) -> bool {
        Primitives::send_response(self, msg);
        false
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) -> bool {
        Primitives::send_response_final(self, msg);
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A mock face attached to a [`Harness`].
///
/// Messages injected via [`MockFace::put`] / [`MockFace::declare_subscriber`] etc. are
/// processed by the gateway's routing tables just like messages from a real session.
/// Messages that the gateway decides to route *to* this face are captured by the internal
/// [`RecordingPrimitives`] and can be inspected with [`MockFace::recorder`].
pub(crate) struct MockFace {
    /// The gateway face handle; used as a `Primitives` source for message injection.
    pub(crate) face: Arc<Face>,
    /// Records every message the gateway routes to this face.
    recorder: Arc<RecordingPrimitives>,
    /// For remote faces: the `DeMux` wrapping this face, used to inject OAM messages via
    /// the `TransportPeerEventHandler` path (OAM is not part of `Primitives`).
    pub(crate) demux: Option<Arc<DeMux>>,
    /// Maintains a strong count > 1 for the the mock transport inner for remote faces, which `Mux`
    /// only holds via a `Weak` reference).
    _data: Option<Arc<MockTransportUnicastInner>>,
}

impl Debug for MockFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.face, f)
    }
}

impl MockFace {
    /// Inject a `Put` with an explicit payload.
    pub(crate) fn put(&self, wire_expr: impl Into<WireExpr<'static>>, payload: Vec<u8>) {
        self.face.send_push(
            &mut Push {
                wire_expr: wire_expr.into(),
                ext_qos: ext::QoSType::DEFAULT,
                ext_tstamp: None,
                ext_nodeid: NodeIdType::DEFAULT,
                payload: PushBody::Put(Put {
                    payload: payload.into(),
                    ..Default::default()
                }),
            },
            Reliability::BestEffort,
        );
    }

    /// Inject a `Get` with an explicit payload.
    pub(crate) fn query(&self, id: RequestId, wire_expr: impl Into<WireExpr<'static>>) {
        self.face.send_request(&mut Request {
            id,
            wire_expr: wire_expr.into(),
            ext_qos: ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            ext_target: QueryTarget::DEFAULT,
            ext_budget: None,
            ext_timeout: None,
            payload: RequestBody::Query(Query::default()),
        });
    }

    /// Inject a `Delete` publication (tombstone) into the gateway.
    pub(crate) fn delete(&self, wire_expr: impl Into<WireExpr<'static>>) {
        use zenoh_protocol::zenoh::{Del, PushBody};
        self.face.send_push(
            &mut Push {
                wire_expr: wire_expr.into(),
                ext_qos: ext::QoSType::DEFAULT,
                ext_tstamp: None,
                ext_nodeid: NodeIdType::DEFAULT,
                payload: PushBody::Del(Del::default()),
            },
            Reliability::BestEffort,
        );
    }

    /// Declare a keyexpr from this face's perspective.
    pub(crate) fn declare_keyexpr(
        &self,
        interest_id: Option<InterestId>,
        id: ExprId,
        wire_expr: impl Into<WireExpr<'static>>,
    ) {
        self.face.send_declare(&mut Declare {
            interest_id,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                id,
                wire_expr: wire_expr.into(),
            }),
        });
    }

    /// Declare a subscriber for `key_expr` from this face's perspective.
    pub(crate) fn declare_subscriber(
        &self,
        interest_id: Option<InterestId>,
        id: SubscriberId,
        wire_expr: impl Into<WireExpr<'static>>,
    ) {
        self.face.send_declare(&mut Declare {
            interest_id,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                id,
                wire_expr: wire_expr.into(),
            }),
        });
    }

    /// Remove a previously-declared subscriber.
    pub(crate) fn undeclare_subscriber(&self, id: SubscriberId) {
        self.face.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                id,
                ext_wire_expr: WireExprType::null(),
            }),
        });
    }

    /// Declare a queryable for `key_expr` from this face's perspective.
    pub(crate) fn declare_queryable(
        &self,
        interest_id: Option<InterestId>,
        id: QueryableId,
        wire_expr: impl Into<WireExpr<'static>>,
    ) {
        use zenoh_protocol::network::declare::queryable::ext::QueryableInfoType;
        self.face.send_declare(&mut Declare {
            interest_id,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                id,
                wire_expr: wire_expr.into(),
                ext_info: QueryableInfoType::DEFAULT,
            }),
        });
    }

    /// Remove a previously-declared queryable.
    pub(crate) fn undeclare_queryable(&self, id: QueryableId) {
        self.face.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                id,
                ext_wire_expr: WireExprType::null(),
            }),
        });
    }

    /// Declare a liveliness token for `key_expr` from this face's perspective.
    pub(crate) fn declare_token(
        &self,
        interest_id: Option<InterestId>,
        id: TokenId,
        wire_expr: impl Into<WireExpr<'static>>,
    ) {
        self.face.send_declare(&mut Declare {
            interest_id,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::DeclareToken(DeclareToken {
                id,
                wire_expr: wire_expr.into(),
            }),
        });
    }

    /// Remove a previously-declared liveliness token.
    pub(crate) fn undeclare_token(&self, id: TokenId) {
        self.face.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::UndeclareToken(UndeclareToken {
                id,
                ext_wire_expr: WireExprType::null(),
            }),
        });
    }

    /// Declare an interest from this face's perspective.
    pub(crate) fn interest(
        &self,
        id: InterestId,
        mode: InterestMode,
        options: InterestOptions,
        wire_expr: impl Into<WireExpr<'static>>,
    ) {
        self.face.send_interest(&mut Interest {
            id,
            mode,
            options,
            wire_expr: Some(wire_expr.into()),
            ext_qos: ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
        });
    }

    /// Declare an interest without a key expression.
    pub(crate) fn interest_wildcard(
        &self,
        id: InterestId,
        mode: InterestMode,
        options: InterestOptions,
    ) {
        self.face.send_interest(&mut Interest {
            id,
            mode,
            options,
            wire_expr: None,
            ext_qos: ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
        });
    }

    /// Declare a liveliness token for `key_expr` from this face's perspective.
    pub(crate) fn declare_final(&self, interest_id: InterestId) {
        self.face.send_declare(&mut Declare {
            interest_id: Some(interest_id), // NOTE: DeclareFinal without interest id is invalid
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: NodeIdType::DEFAULT,
            body: DeclareBody::DeclareFinal(DeclareFinal),
        });
    }

    /// Access the recorder to inspect what the gateway has delivered to this face.
    pub(crate) fn recorder(&self) -> &RecordingPrimitives {
        &self.recorder
    }
}

/// Builder for [`Harness`].
///
/// Obtain one via [`HarnessBuilder::new`] and call [`HarnessBuilder::build`] to construct
/// the [`Harness`]. All fields have defaults identical to those previously used by
/// `Harness::with_subregions`: router mode, no custom ZID, no subregions, runtime enabled,
/// adminspace disabled.
pub(crate) struct HarnessBuilder {
    mode: WhatAmI,
    zid: Option<ZenohId>,
    subregions: Vec<Region>,
    start_runtime: bool,
    start_adminspace: bool,
}

impl HarnessBuilder {
    pub(crate) fn new() -> Self {
        Self {
            mode: WhatAmI::default(),
            zid: None,
            subregions: Vec::new(),
            start_runtime: true,
            start_adminspace: false,
        }
    }

    /// Set the [`WhatAmI`] mode of this harness.
    pub(crate) fn mode(mut self, mode: WhatAmI) -> Self {
        self.mode = mode;
        self
    }

    /// Assign a specific [`ZenohId`] to this harness's runtime.
    pub(crate) fn zid(mut self, zid: ZenohId) -> Self {
        self.zid = Some(zid);
        self
    }

    /// Set the subregions the gateway will route across.
    pub(crate) fn subregions(mut self, subregions: impl Into<Vec<Region>>) -> Self {
        self.subregions = subregions.into();
        self
    }

    /// Control whether a real [`Runtime`] is started for this harness (default: `true`).
    ///
    /// When `false`, a [`GatewayBuilder`] is used directly and no runtime overhead is
    /// incurred. This is equivalent to the old `Harness::with_subregions_noruntime`.
    pub(crate) fn start_runtime(mut self, start_runtime: bool) -> Self {
        self.start_runtime = start_runtime;
        self
    }

    /// Control whether the admin space is enabled in the runtime config (default: `false`).
    ///
    /// Only meaningful when [`start_runtime`](Self::start_runtime) is `true`.
    pub(crate) fn start_adminspace(mut self, start_adminspace: bool) -> Self {
        self.start_adminspace = start_adminspace;
        self
    }

    /// Consume this builder and produce a [`Harness`].
    pub(crate) fn build(self) -> Harness {
        if self.start_runtime {
            let mut config = Config::default();
            if let Some(zid) = self.zid {
                config.set_id(Some(zid)).unwrap();
            }
            config.set_mode(Some(self.mode)).unwrap();

            // NOTE(regions): these lines attempt to remove all side-effects of creating a runtime.
            config.listen.endpoints.set(vec![]).unwrap();
            config.connect.endpoints.set(vec![]).unwrap();
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
            config
                .adminspace
                .set_enabled(self.start_adminspace)
                .unwrap();
            config.plugins_loading.set_enabled(false).unwrap();

            let runtime = block_on(
                RuntimeBuilder::new(crate::api::config::Config(config))
                    .subregions(self.subregions)
                    .disable_async_tree_computation(true)
                    .build(),
            )
            .unwrap();
            let gateway = Gateway {
                tables: runtime.router().tables.clone(),
            };
            Harness {
                gateway,
                zid: runtime.zid().into(),
                _runtime: Some(runtime),
            }
        } else {
            let mut config = Config::default().expanded();
            if let Some(zid) = self.zid {
                config.set_id(Some(zid)).unwrap();
            }
            config.set_mode(Some(self.mode)).unwrap();
            let gateway = GatewayBuilder::new(&config)
                .subregions(self.subregions)
                .disable_async_tree_computation(true)
                .build()
                .unwrap();
            Harness {
                gateway,
                zid: config.id().into(),
                _runtime: None,
            }
        }
    }
}

/// Wraps a [`Gateway`] and provides ergonomic helpers for unit tests.
pub(crate) struct Harness {
    pub(crate) gateway: Gateway,
    zid: ZenohIdProto,
    /// Kept alive so that the runtime's transport manager and background tasks outlive the harness.
    /// `None` for client/peer harnesses that don't need a runtime.
    _runtime: Option<Runtime>,
}

impl Harness {
    const DEFAULT_SUBREGIONS: [Region; 4] = [
        Region::Local,
        Region::default_south(WhatAmI::Client),
        Region::default_south(WhatAmI::Peer),
        Region::default_south(WhatAmI::Router),
    ];

    pub(crate) fn new_client() -> Self {
        HarnessBuilder::new()
            .mode(WhatAmI::Client)
            .subregions(Self::DEFAULT_SUBREGIONS)
            .build()
    }

    pub(crate) fn new_peer() -> Self {
        HarnessBuilder::new()
            .mode(WhatAmI::Peer)
            .subregions(Self::DEFAULT_SUBREGIONS)
            .build()
    }

    /// Build a gateway in router mode.
    ///
    /// Uses a real [`Runtime`] so that [`Gateway::init_hats`] is called — required for the
    /// router hat's topology tracking to function correctly.
    pub(crate) fn new_router() -> Self {
        HarnessBuilder::new()
            .mode(WhatAmI::Router)
            .subregions(Self::DEFAULT_SUBREGIONS)
            .build()
    }

    pub(crate) fn zid(&self) -> ZenohIdProto {
        self.zid
    }

    pub(crate) fn new_session(&self) -> MockFace {
        let recorder = Arc::new(RecordingPrimitives::new());
        let face = self.gateway.new_session(recorder.clone());
        MockFace {
            face,
            recorder,
            demux: None,
            _data: None,
        }
    }

    /// Attach a mock face to the gateway and return its handle.
    ///
    /// For `Region::Local`, this calls [`Gateway::new_primitives`] directly.
    /// For all other regions, a mock transport unicast is used.
    pub(crate) fn new_face(&self, cfg: FaceDef) -> MockFace {
        let recorder = Arc::new(RecordingPrimitives::new());

        assert_ne!(cfg.region, Region::Local);

        use zenoh_transport::unicast::test_helpers::mock_transport_unicast;

        let rec = recorder.clone();
        let (mock_transport, mock_inner) = mock_transport_unicast(
            cfg.zid,
            cfg.mode,
            Arc::new(move |msg| match msg.body {
                NetworkBody::Push(mut p) => {
                    Primitives::send_push_consume(&*rec, &mut p, msg.reliability, true)
                }
                NetworkBody::Declare(mut d) => Primitives::send_declare(&*rec, &mut d),
                NetworkBody::Interest(mut i) => Primitives::send_interest(&*rec, &mut i),
                NetworkBody::Request(mut r) => Primitives::send_request(&*rec, &mut r),
                NetworkBody::Response(mut r) => Primitives::send_response(&*rec, &mut r),
                NetworkBody::ResponseFinal(mut r) => Primitives::send_response_final(&*rec, &mut r),
                NetworkBody::OAM(o) => rec.messages.lock().unwrap().push(Message::Oam(o)),
            }),
        );

        let demux = self
            .gateway
            .new_transport_unicast(mock_transport, cfg.region, cfg.remote_bound)
            .unwrap();

        let face = Arc::new(demux.face.clone());
        MockFace {
            face,
            recorder,
            demux: Some(demux),
            // Keep the mock transport inner alive: Mux (inside FaceState::primitives)
            // only holds a Weak to the transport, so we must maintain a strong Arc here.
            _data: Some(mock_inner),
        }
    }
}

/// Configures a face added to a [`Harness`] via [`Harness::new_face`].
#[derive(Default, Clone, Copy)]
pub(crate) struct FaceDef {
    pub(crate) region: Region,
    pub(crate) mode: WhatAmI,
    pub(crate) remote_bound: Bound,
    pub(crate) zid: ZenohIdProto,
}

impl FaceDef {
    pub(crate) fn region(mut self, region: Region) -> Self {
        self.region = region;
        self
    }

    pub(crate) fn mode(mut self, mode: WhatAmI) -> Self {
        self.mode = mode;
        self
    }

    pub(crate) fn remote_bound(mut self, remote_bound: Bound) -> Self {
        self.remote_bound = remote_bound;
        self
    }

    pub(crate) fn zid(mut self, zid: ZenohIdProto) -> Self {
        self.zid = zid;
        self
    }
}

/// Connects two gateways together at the primitives level.
#[derive(Debug)]
pub(crate) struct EstablishedConnection {
    /// A's face for B.
    pub(crate) a2b: MockFace,
    /// B's face for A.
    pub(crate) b2a: MockFace,
    nfwd: usize,
    rev_nfwd: usize,
}

/// A mock of a network connection from [`Connection::a`] to [`Connection::b`].
pub(crate) struct Connection<'a> {
    /// Gateway A.
    pub(crate) a: &'a Harness,
    /// A's face for B.
    pub(crate) a2b: FaceDef,
    /// Gateway B.
    pub(crate) b: &'a Harness,
    /// B's face for A.
    pub(crate) b2a: FaceDef,
}

impl Connection<'_> {
    pub(crate) fn establish(self) -> EstablishedConnection {
        EstablishedConnection {
            a2b: self.a.new_face(self.a2b.zid(self.b.zid())),
            b2a: self.b.new_face(self.b2a.zid(self.a.zid())),
            nfwd: 0,
            rev_nfwd: 0,
        }
    }
}

impl EstablishedConnection {
    /// Forward one pending message **from A to B**.
    #[tracing::instrument(level = "info", skip(self), fields(from = %self.b2a.face, to = %self.a2b.face.state.zid.short()), ret)]
    pub(crate) fn fwd1(&mut self) -> Option<Message> {
        let msg = self
            .a2b
            .recorder
            .with_messages(|msgs| msgs.get(self.nfwd).cloned())?;
        Self::inject(&self.b2a, &msg);
        self.nfwd += 1;
        Some(msg)
    }

    /// Forward one pending message in the reverse direction, i.e. **from B to A**.
    #[tracing::instrument(level = "info", skip(self), fields(from = %self.a2b.face, to = %self.b2a.face.state.zid.short()), ret)]
    pub(crate) fn rev_fwd1(&mut self) -> Option<Message> {
        let msg = self
            .b2a
            .recorder
            .with_messages(|msgs| msgs.get(self.rev_nfwd).cloned())?;
        Self::inject(&self.a2b, &msg);
        self.rev_nfwd += 1;
        Some(msg)
    }

    /// Forward all pending messages **from A to B**.
    pub(crate) fn fwd(&mut self) -> Vec<Message> {
        self.a2b.recorder().with_messages(|msgs| {
            let mut out = Vec::new();

            while let Some(msg) = msgs.get(self.nfwd).cloned() {
                Self::inject(&self.b2a, &msg);
                self.nfwd += 1;
                out.push(msg);
            }

            out
        })
    }

    /// Forward one pending message in the reverse direction, i.e. **from B to A**.
    pub(crate) fn rev_fwd(&mut self) -> Vec<Message> {
        self.b2a.recorder().with_messages(|msgs| {
            let mut out = Vec::new();

            while let Some(msg) = msgs.get(self.rev_nfwd).cloned() {
                Self::inject(&self.a2b, &msg);
                self.rev_nfwd += 1;
                out.push(msg);
            }

            out
        })
    }

    /// Bi-directionally forward all pending messages, i.e. **from A to B and from B to A**.
    pub(crate) fn bi_fwd(&mut self) {
        loop {
            match (self.fwd1(), self.rev_fwd1()) {
                (None, None) => break,
                _ => continue,
            }
        }
    }

    pub(crate) fn bi_fwd_many_bounded<const N: usize, const L: usize>(
        mut conns: [&mut EstablishedConnection; N],
    ) {
        for _ in 0..L {
            if conns
                .iter_mut()
                .all(|c| c.fwd1().is_none() && c.rev_fwd1().is_none())
            {
                break;
            }
        }
    }

    pub(crate) fn bi_fwd_many_unbounded<const N: usize>(
        mut conns: [&mut EstablishedConnection; N],
    ) {
        loop {
            if conns
                .iter_mut()
                .all(|c| c.fwd1().is_none() && c.rev_fwd1().is_none())
            {
                break;
            }
        }
    }

    #[tracing::instrument(level = "info", fields(target = %target.face), ret)]
    fn inject(target: &MockFace, msg: &Message) {
        match msg {
            Message::Push(p) => target
                .face
                .send_push(&mut p.clone(), Reliability::default()),
            Message::Declare(d) => target.face.send_declare(&mut d.clone()),
            Message::Request(r) => target.face.send_request(&mut r.clone()),
            Message::Response(r) => target.face.send_response(&mut r.clone()),
            Message::ResponseFinal(r) => target.face.send_response_final(&mut r.clone()),
            Message::Interest(i) => target.face.send_interest(&mut i.clone()),
            Message::Oam(o) => {
                if let Some(demux) = &target.demux {
                    let _ = demux.handle_message(NetworkMessageMut {
                        body: NetworkBodyMut::OAM(&mut o.clone()),
                        reliability: Reliability::default(),
                    });
                }
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.a2b
            .recorder
            .with_messages(|msgs| msgs.len() == self.nfwd)
    }

    fn is_rev_complete(&self) -> bool {
        self.b2a
            .recorder
            .with_messages(|msgs| msgs.len() == self.rev_nfwd)
    }

    fn is_bi_complete(&self) -> bool {
        self.is_complete() && self.is_rev_complete()
    }
}
