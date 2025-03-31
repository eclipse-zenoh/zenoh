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
use std::sync::OnceLock;

use arc_swap::ArcSwap;
use zenoh_protocol::{
    core::Reliability,
    network::{
        interest::Interest, response, Declare, NetworkBody, NetworkMessage, Push, Request,
        Response, ResponseFinal,
    },
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{EPrimitives, Primitives};
use crate::net::routing::{
    dispatcher::face::{Face, WeakFace},
    interceptor::{InterceptorTrait, InterceptorsChain},
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

impl EPrimitives for Mux {
    fn send_interest(&self, ctx: RoutingContext<Interest>) {
        let interest_id = ctx.msg.id;
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Interest(ctx.msg),
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
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap(), &interceptor));

        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());

        match interceptor.intercept(ctx, cache) {
            Some(ctx) => {
                let _ = self.handler.schedule(ctx.msg);
            }
            None => {
                // send declare final to avoid timeout on blocked interest
                if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
                    face.reject_interest(interest_id);
                }
            }
        };
    }

    fn send_declare(&self, ctx: RoutingContext<Declare>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Declare(ctx.msg),
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
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap(), &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        if let Some(ctx) = interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: Push, reliability: Reliability) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            reliability,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());

            if let Some(ctx) = interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, msg: Request) {
        let request_id = msg.id;
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());

            match interceptor.intercept(ctx, cache) {
                Some(ctx) => {
                    let _ = self.handler.schedule(ctx.msg);
                }
                None => {
                    // request was blocked by an interceptor, we need to send response final to avoid timeout error
                    face.send_response_final(ResponseFinal {
                        rid: request_id,
                        ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                        ext_tstamp: None,
                    })
                }
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(&face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
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
    fn send_interest(&self, ctx: RoutingContext<Interest>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Interest(ctx.msg),
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
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap(), &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        if let Some(ctx) = interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_declare(&self, ctx: RoutingContext<Declare>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Declare(ctx.msg),
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
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap(), &interceptor));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        if let Some(ctx) = interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: Push, reliability: Reliability) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            reliability,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            tracing::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            reliability: Reliability::Reliable,
            #[cfg(feature = "stats")]
            size: None,
        };
        let interceptor = self.interceptor.load();
        if interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache_guard = prefix
                .as_ref()
                .and_then(|p| p.get_egress_cache(face, &interceptor));
            let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
            if let Some(ctx) = interceptor.intercept(ctx, cache) {
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
