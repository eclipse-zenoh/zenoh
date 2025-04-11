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
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use zenoh_protocol::{
    core::Reliability,
    network::{
        interest::Interest, Declare, NetworkBodyMut, NetworkMessageMut, Push, Request, Response,
        ResponseFinal,
    },
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::EPrimitives;
use crate::net::routing::{
    dispatcher::face::{Face, WeakFace},
    interceptor::{InterceptorTrait, InterceptorsChain},
    RoutingContext,
};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) face: OnceLock<WeakFace>,
    pub(crate) interceptor: Arc<ArcSwap<InterceptorsChain>>,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast, interceptor: InterceptorsChain) -> Mux {
        Mux {
            handler,
            face: OnceLock::new(),
            interceptor: Arc::new(ArcSwap::new(interceptor.into())),
        }
    }
}

impl EPrimitives for Mux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let interest_id = ctx.msg.id;
        let mut ctx = RoutingContext {
            msg: NetworkMessageMut {
                body: NetworkBodyMut::Interest(ctx.msg),
                reliability: Reliability::Reliable,
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        let prefix = ctx
            .wire_expr()
            .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
            .flatten()
            .cloned();
        let interceptor = self.interceptor.load();
        let cache_guard = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(&ctx.outface.get().unwrap().state, &interceptor));

        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());

        if self.interceptor.load().intercept(&mut ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        } else {
            // send declare final to avoid timeout on blocked interest
            if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
                face.reject_interest(interest_id);
            }
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let mut ctx = RoutingContext {
            msg: NetworkMessageMut {
                body: NetworkBodyMut::Declare(ctx.msg),
                reliability: Reliability::Reliable,
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        let prefix = ctx
            .wire_expr()
            .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
            .flatten()
            .cloned();
        let interceptor = self.interceptor.load();
        let cache_guard = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(&ctx.outface.get().unwrap().state, &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());

        if self.interceptor.load().intercept(&mut ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
            #[cfg(feature = "stats")]
            size: None,
        };

        let _ = self.handler.schedule(msg);
    }

    fn send_request(&self, msg: &mut Request) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };

        let _ = self.handler.schedule(msg);
    }

    fn send_response(&self, msg: &mut Response) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let _ = self.handler.schedule(msg);
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

impl EPrimitives for McastMux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let mut ctx = RoutingContext {
            msg: NetworkMessageMut {
                body: NetworkBodyMut::Interest(ctx.msg),
                reliability: Reliability::Reliable,
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        let prefix = ctx
            .wire_expr()
            .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
            .flatten()
            .cloned();
        let interceptor = self.interceptor.load();
        let cache_guard = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(&ctx.outface.get().unwrap().state, &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        if self.interceptor.load().intercept(&mut ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let mut ctx = RoutingContext {
            msg: NetworkMessageMut {
                body: NetworkBodyMut::Declare(ctx.msg),
                reliability: Reliability::Reliable,
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        let prefix = ctx
            .wire_expr()
            .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
            .flatten()
            .cloned();
        let interceptor = self.interceptor.load();
        let cache_guard = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(&ctx.outface.get().unwrap().state, &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        if self.interceptor.load().intercept(&mut ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let mut ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face.state, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if interceptor.intercept(&mut ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, msg: &mut Request) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let mut ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face.state, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if interceptor.intercept(&mut ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: &mut Response) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let mut ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face.state, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if interceptor.intercept(&mut ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let mut ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face.state, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if interceptor.intercept(&mut ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
