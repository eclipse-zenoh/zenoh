use std::sync::Arc;

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
    dispatcher::{face::Face, tables::TablesLock},
    interceptor::{InterceptTrait, InterceptsChain},
    RoutingContext,
};
use zenoh_protocol::network::{
    Declare, NetworkBody, NetworkMessage, Push, Request, Response, ResponseFinal,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) fid: usize,
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) intercept: InterceptsChain,
}

impl Mux {
    pub(crate) fn new(
        handler: TransportUnicast,
        fid: usize,
        tables: Arc<TablesLock>,
        intercept: InterceptsChain,
    ) -> Mux {
        Mux {
            handler,
            fid,
            tables,
            intercept,
        }
    }
}

impl Primitives for Mux {
    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        let tables = zread!(self.tables.tables);
        let face = tables.faces.get(&self.fid).cloned();
        drop(tables);
        if let Some(face) = face {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        let tables = zread!(self.tables.tables);
        let face = tables.faces.get(&self.fid).cloned();
        drop(tables);
        if let Some(face) = face {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        let tables = zread!(self.tables.tables);
        let face = tables.faces.get(&self.fid).cloned();
        drop(tables);
        if let Some(face) = face {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        let tables = zread!(self.tables.tables);
        let face = tables.faces.get(&self.fid).cloned();
        drop(tables);
        if let Some(face) = face {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        let tables = zread!(self.tables.tables);
        let face = tables.faces.get(&self.fid).cloned();
        drop(tables);
        if let Some(face) = face {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, ctx: RoutingContext<Push>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Push(ctx.msg),
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) fid: usize,
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) intercept: InterceptsChain,
}

impl McastMux {
    pub(crate) fn new(
        handler: TransportMulticast,
        fid: usize,
        tables: Arc<TablesLock>,
        intercept: InterceptsChain,
    ) -> McastMux {
        McastMux {
            handler,
            fid,
            tables,
            intercept,
        }
    }
}

impl Primitives for McastMux {
    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(face) = zread!(self.tables.tables).faces.get(&self.fid).cloned() {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(face) = zread!(self.tables.tables).faces.get(&self.fid).cloned() {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(face) = zread!(self.tables.tables).faces.get(&self.fid).cloned() {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(face) = zread!(self.tables.tables).faces.get(&self.fid).cloned() {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(face) = zread!(self.tables.tables).faces.get(&self.fid).cloned() {
            let ctx = RoutingContext::new_in(
                msg,
                Face {
                    tables: self.tables.clone(),
                    state: face.clone(),
                },
            );
            if let Some(ctx) = self.intercept.intercept(ctx) {
                let _ = self.handler.schedule(ctx.msg);
            }
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_push(&self, ctx: RoutingContext<Push>) {
        let ctx = RoutingContext {
            msg: NetworkMessage {
                body: NetworkBody::Push(ctx.msg),
                #[cfg(feature = "stats")]
                size: None,
            },
            inface: ctx.inface,
            outface: ctx.outface,
            prefix: ctx.prefix,
            full_expr: ctx.full_expr,
        };
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
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
        if let Some(ctx) = self.intercept.intercept(ctx) {
            let _ = self.handler.schedule(ctx.msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
