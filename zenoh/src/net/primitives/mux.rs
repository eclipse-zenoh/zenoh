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
use super::{EPrimitives, Primitives};
use crate::net::routing::{
    dispatcher::face::{Face, WeakFace},
    interceptor::{InterceptorTrait, InterceptorsChain},
    RoutingContext,
};
use std::sync::OnceLock;
use zenoh_protocol::network::{
    interest::Interest, Declare, NetworkBody, NetworkMessage, Push, Request, Response,
    ResponseFinal,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) face: OnceLock<WeakFace>,
    pub(crate) interceptor: InterceptorsChain,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast, interceptor: InterceptorsChain) -> Mux {
        Mux {
            handler,
            face: OnceLock::new(),
            interceptor,
        }
    }
}

impl Primitives for Mux {
    fn send_interest(&self, msg: Interest) {
        let msg = NetworkMessage {
            body: NetworkBody::Interest(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}

impl EPrimitives for Mux {
    fn send_declare(&self, ctx: RoutingContext<Declare>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Declare(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get().and_then(|f| f.upgrade()) {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(&face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, ctx: RoutingContext<Request>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Request(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_response(&self, ctx: RoutingContext<Response>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Response(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_response_final(&self, ctx: RoutingContext<ResponseFinal>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::ResponseFinal(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) face: OnceLock<Face>,
    pub(crate) interceptor: InterceptorsChain,
}

impl McastMux {
    pub(crate) fn new(handler: TransportMulticast, interceptor: InterceptorsChain) -> McastMux {
        McastMux {
            handler,
            face: OnceLock::new(),
            interceptor,
        }
    }
}

impl Primitives for McastMux {
    fn send_interest(&self, msg: Interest) {
        let msg = NetworkMessage {
            body: NetworkBody::Interest(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}

impl EPrimitives for McastMux {
    fn send_declare(&self, ctx: RoutingContext<Declare>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Declare(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if self.interceptor.interceptors.is_empty() {
            let _ = self.handler.schedule(msg);
        } else if let Some(face) = self.face.get() {
            let ctx = RoutingContext::new_out(msg, face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix.as_ref().and_then(|p| p.get_egress_cache(face));
            if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
                let _ = self.handler.schedule(ctx.msg);
            }
        } else {
            log::error!("Uninitialized multiplexer!");
        }
    }

    fn send_request(&self, ctx: RoutingContext<Request>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Request(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_response(&self, ctx: RoutingContext<Response>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Response(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_response_final(&self, ctx: RoutingContext<ResponseFinal>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::ResponseFinal(ctx.msg),
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
        let cache = prefix
            .as_ref()
            .and_then(|p| p.get_egress_cache(ctx.outface.get().unwrap()));
        if let Some(ctx) = self.interceptor.intercept(ctx, cache) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
