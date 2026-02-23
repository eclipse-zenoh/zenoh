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

use arc_swap::ArcSwapOption;
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
    interceptor::{has_interceptor, InterceptorContext, InterceptorTrait, InterceptorsChain},
    router::{InterceptorCacheValueType, Resource},
    RoutingContext,
};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) interceptor: ArcSwapOption<InterceptorsChain>,
    pub(crate) face: OnceLock<WeakFace>,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast, interceptor: InterceptorsChain) -> Mux {
        Mux {
            handler,
            face: OnceLock::new(),
            interceptor: ArcSwapOption::new(interceptor.into()),
        }
    }

    #[inline(always)]
    fn can_schedule(&self, msg: &mut NetworkMessageMut) -> bool {
        if !has_interceptor(&self.interceptor) {
            return true;
        }
        match self.interceptor.load().as_ref() {
            Some(interceptor) => interceptor.intercept(
                msg,
                &mut MuxContext {
                    mux: self,
                    cache: OnceCell::new(),
                    expr: OnceCell::new(),
                },
            ),
            None => true,
        }
    }

    #[inline(always)]
    fn schedule(&self, mut msg: NetworkMessageMut) -> bool {
        self.can_schedule(&mut msg) && self.handler.schedule(msg).unwrap_or(false)
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
                    // TODO interceptor can change between the initial load and the cache load
                    if let Some(cache) = self
                        .mux
                        .interceptor
                        .load()
                        .as_ref()
                        .and_then(|i| prefix.get_egress_cache(&face, i))
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
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) -> bool {
        let interest_id = ctx.msg.id;

        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self.interceptor.load().intercept(&mut msg, &mut ctx) {
            self.handler.schedule(msg).unwrap_or(false)
        } else {
            // send declare final to avoid timeout on blocked interest
            if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
                face.reject_interest(interest_id);
            }
            false
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) -> bool {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        self.interceptor.load().intercept(&mut msg, &mut ctx)
            && self.handler.schedule(msg).unwrap_or(false)
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };
        self.schedule(msg)
    }

    fn send_request(&self, msg: &mut Request) -> bool {
        let request_id = msg.id;
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };
        if self.can_schedule(&mut msg) {
            self.handler.schedule(msg).unwrap_or(false)
        } else {
            match self.face.get().and_then(|f| f.upgrade()) {
                Some(face) => face.send_response_final(&mut ResponseFinal {
                    rid: request_id,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                }),
                None => tracing::error!("Uninitialized multiplexer!"),
            }
            false
        }
    }

    fn send_response(&self, msg: &mut Response) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        self.schedule(msg)
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        self.schedule(msg)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) face: OnceLock<Face>,
    pub(crate) interceptor: ArcSwapOption<InterceptorsChain>,
}

impl McastMux {
    pub(crate) fn new(handler: TransportMulticast, interceptor: InterceptorsChain) -> McastMux {
        McastMux {
            handler,
            face: OnceLock::new(),
            interceptor: ArcSwapOption::new(interceptor.into()),
        }
    }

    #[inline(always)]
    fn can_schedule(&self, msg: &mut NetworkMessageMut) -> bool {
        match self.interceptor.load().as_ref() {
            Some(interceptor) => interceptor.intercept(
                msg,
                &mut McastMuxContext {
                    mux: self,
                    cache: OnceCell::new(),
                    expr: OnceCell::new(),
                },
            ),
            None => true,
        }
    }

    #[inline(always)]
    fn schedule(&self, mut msg: NetworkMessageMut) -> bool {
        self.can_schedule(&mut msg) && self.handler.schedule(msg).unwrap_or(false)
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
                    // TODO interceptor can change between the initial load and the cache load
                    if let Some(cache) = self
                        .mux
                        .interceptor
                        .load()
                        .as_ref()
                        .and_then(|i| prefix.get_egress_cache(face, i))
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
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) -> bool {
        let interest_id = ctx.msg.id;

        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self.interceptor.load().intercept(&mut msg, &mut ctx) {
            self.handler.schedule(msg).unwrap_or(false)
        } else {
            // send declare final to avoid timeout on blocked interest
            if let Some(face) = self.face.get() {
                face.reject_interest(interest_id);
            }
            false
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) -> bool {
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let mut ctx = RoutingContext {
            msg: (),
            full_expr: ctx.full_expr,
        };

        if self.interceptor.load().intercept(&mut msg, &mut ctx) {
            self.handler.schedule(msg).unwrap_or(false)
        } else {
            false
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };
        self.schedule(msg)
    }

    fn send_request(&self, msg: &mut Request) -> bool {
        let request_id = msg.id;
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };
        if self.can_schedule(&mut msg) {
            self.handler.schedule(msg).unwrap_or(false)
        } else {
            match self.face.get() {
                Some(face) => face.send_response_final(&mut ResponseFinal {
                    rid: request_id,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                }),
                None => tracing::error!("Uninitialized multiplexer!"),
            }
            false
        }
    }

    fn send_response(&self, msg: &mut Response) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        self.schedule(msg)
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) -> bool {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        self.schedule(msg)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
