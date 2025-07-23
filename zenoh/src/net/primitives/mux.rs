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
    any::Any,
    cell::OnceCell,
    sync::{Arc, OnceLock},
};

use arc_swap::ArcSwap;
use zenoh_protocol::{
    core::Reliability,
    network::{
        interest::Interest, response, Declare, NetworkBodyMut, NetworkMessageExt as _,
        NetworkMessageMut, Push, Request, Response, ResponseFinal,
    },
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{EPrimitives, Primitives};
use crate::net::routing::{
    dispatcher::face::{Face, WeakFace},
    interceptor::{InterceptorContext, InterceptorTrait, InterceptorsChain},
    router::{InterceptorCacheValueType, Resource},
    RoutingContext,
};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) face: OnceLock<WeakFace>,
    pub(crate) interceptor: ArcSwap<InterceptorsChain>,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast, interceptor: InterceptorsChain) -> Mux {
        Mux {
            handler,
            face: OnceLock::new(),
            interceptor: ArcSwap::new(interceptor.into()),
        }
    }
}

struct MuxContext<'a> {
    mux: &'a Mux,
    cache: OnceCell<InterceptorCacheValueType>,
    expr: OnceCell<String>,
}

impl MuxContext<'_> {
    fn prefix(&self, msg: &NetworkMessageMut) -> Option<Arc<Resource>> {
        if let Some(wire_expr) = msg.wire_expr() {
            let wire_expr = wire_expr.to_owned();
            if let Some(face) = self.mux.face.get().and_then(|f| f.upgrade()) {
                if let Some(prefix) = zread!(face.tables.tables)
                    .get_sent_mapping(&face.state, &wire_expr.scope, wire_expr.mapping)
                    .cloned()
                {
                    return Some(prefix);
                }
            }
        }
        None
    }
}

impl InterceptorContext for MuxContext<'_> {
    fn face(&self) -> Option<Face> {
        self.mux.face.get().and_then(|f| f.upgrade())
    }

    fn full_expr(&self, msg: &NetworkMessageMut) -> Option<&str> {
        if self.expr.get().is_none() {
            if let Some(wire_expr) = msg.wire_expr() {
                if let Some(prefix) = self.prefix(msg) {
                    self.expr
                        .set(prefix.expr().to_string() + wire_expr.suffix.as_ref())
                        .ok();
                }
            }
        }
        self.expr.get().map(|x| x.as_str())
    }
    fn get_cache(&self, msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
        if self.cache.get().is_none() && msg.wire_expr().is_some_and(|we| !we.has_suffix()) {
            if let Some(prefix) = self.prefix(msg) {
                if let Some(face) = self.mux.face.get().and_then(|f| f.upgrade()) {
                    if let Some(cache) =
                        prefix.get_egress_cache(&face, &self.mux.interceptor.load())
                    {
                        self.cache.set(cache).ok();
                    }
                }
            }
        }
        self.cache.get().and_then(|c| c.get_ref().as_ref())
    }
}

impl EPrimitives for Mux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let interest_id = ctx.msg.id;

        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self
            .interceptor
            .load()
            .intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        } else {
            // send declare final to avoid timeout on blocked interest
            if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
                face.reject_interest(interest_id);
            }
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self
            .interceptor
            .load()
            .intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };
        let mut ctx = MuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_request(&self, msg: &mut Request) {
        let request_id = msg.id;
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = MuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            if interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext) {
                let _ = self.handler.schedule(msg);
            } else {
                // request was blocked by an interceptor, we need to send response final to avoid timeout error
                face.send_response_final(&mut ResponseFinal {
                    rid: request_id,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                })
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: &mut Response) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = MuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = MuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) face: OnceLock<Face>,
    pub(crate) interceptor: ArcSwap<InterceptorsChain>,
}

impl McastMux {
    pub(crate) fn new(handler: TransportMulticast, interceptor: InterceptorsChain) -> McastMux {
        McastMux {
            handler,
            face: OnceLock::new(),
            interceptor: ArcSwap::new(interceptor.into()),
        }
    }
}

struct McastMuxContext<'a> {
    mux: &'a McastMux,
    cache: OnceCell<InterceptorCacheValueType>,
    expr: OnceCell<String>,
}

impl McastMuxContext<'_> {
    fn prefix(&self, msg: &NetworkMessageMut) -> Option<Arc<Resource>> {
        if let Some(wire_expr) = msg.wire_expr() {
            let wire_expr = wire_expr.to_owned();
            if let Some(face) = self.mux.face.get() {
                if let Some(prefix) = zread!(face.tables.tables)
                    .get_sent_mapping(&face.state, &wire_expr.scope, wire_expr.mapping)
                    .cloned()
                {
                    return Some(prefix);
                }
            }
        }
        None
    }
}

impl InterceptorContext for McastMuxContext<'_> {
    fn face(&self) -> Option<Face> {
        self.mux.face.get().cloned()
    }

    fn full_expr(&self, msg: &NetworkMessageMut) -> Option<&str> {
        if self.expr.get().is_none() {
            if let Some(wire_expr) = msg.wire_expr() {
                if let Some(prefix) = self.prefix(msg) {
                    self.expr
                        .set(prefix.expr().to_string() + wire_expr.suffix.as_ref())
                        .ok();
                }
            }
        }
        self.expr.get().map(|x| x.as_str())
    }
    fn get_cache(&self, msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
        if self.cache.get().is_none() && msg.wire_expr().is_some_and(|we| !we.has_suffix()) {
            if let Some(prefix) = self.prefix(msg) {
                if let Some(face) = self.mux.face.get() {
                    if let Some(cache) = prefix.get_egress_cache(face, &self.mux.interceptor.load())
                    {
                        self.cache.set(cache).ok();
                    }
                }
            }
        }
        self.cache.get().and_then(|c| c.get_ref().as_ref())
    }
}

impl EPrimitives for McastMux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let interest_id = ctx.msg.id;

        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self
            .interceptor
            .load()
            .intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        } else {
            // send declare final to avoid timeout on blocked interest
            if let Some(face) = self.face.get() {
                face.reject_interest(interest_id);
            }
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self
            .interceptor
            .load()
            .intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };
        let mut ctx = McastMuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_request(&self, msg: &mut Request) {
        let request_id = msg.id;
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = McastMuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            if interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext) {
                let _ = self.handler.schedule(msg);
            } else {
                // request was blocked by an interceptor, we need to send response final to avoid timeout error
                face.send_response_final(&mut ResponseFinal {
                    rid: request_id,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                })
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: &mut Response) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = McastMuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = McastMuxContext {
            mux: self,
            cache: OnceCell::new(),
            expr: OnceCell::new(),
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty()
            || interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext)
        {
            let _ = self.handler.schedule(msg);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
