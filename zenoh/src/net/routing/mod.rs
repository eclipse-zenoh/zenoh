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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
pub mod dispatcher;
pub mod hat;
pub mod interceptor;
pub mod router;

use std::{cell::OnceCell, sync::Arc};

use zenoh_protocol::{core::WireExpr, network::NetworkMessage};

use self::{dispatcher::face::Face, router::Resource};

use super::runtime;

pub(crate) static PREFIX_LIVELINESS: &str = "@/liveliness";

pub(crate) struct RoutingContext<Msg> {
    pub(crate) msg: Msg,
    pub(crate) inface: OnceCell<Face>,
    pub(crate) prefix: OnceCell<Arc<Resource>>,
    pub(crate) full_expr: OnceCell<String>,
}

impl<Msg> RoutingContext<Msg> {
    pub(crate) fn with_face(msg: Msg, inface: Face) -> Self {
        Self {
            msg,
            inface: OnceCell::from(inface),
            prefix: OnceCell::new(),
            full_expr: OnceCell::new(),
        }
    }

    pub(crate) fn with_expr(msg: Msg, expr: String) -> Self {
        Self {
            msg,
            inface: OnceCell::new(),
            prefix: OnceCell::new(),
            full_expr: OnceCell::from(expr),
        }
    }

    pub(crate) fn inface(&self) -> Option<&Face> {
        self.inface.get()
    }
}

impl RoutingContext<NetworkMessage> {
    #[inline]
    pub(crate) fn wire_expr(&self) -> Option<&WireExpr> {
        use zenoh_protocol::network::DeclareBody;
        use zenoh_protocol::network::NetworkBody;
        match &self.msg.body {
            NetworkBody::Push(m) => Some(&m.wire_expr),
            NetworkBody::Request(m) => Some(&m.wire_expr),
            NetworkBody::Response(m) => Some(&m.wire_expr),
            NetworkBody::ResponseFinal(_) => None,
            NetworkBody::Declare(m) => match &m.body {
                DeclareBody::DeclareKeyExpr(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareKeyExpr(_) => None,
                DeclareBody::DeclareSubscriber(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareSubscriber(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareQueryable(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareQueryable(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareToken(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareToken(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareInterest(m) => Some(&m.wire_expr),
                DeclareBody::FinalInterest(_) => None,
                DeclareBody::UndeclareInterest(m) => Some(&m.ext_wire_expr.wire_expr),
            },
            NetworkBody::OAM(_) => None,
        }
    }

    #[inline]
    pub(crate) fn full_expr(&self) -> Option<&str> {
        if self.full_expr.get().is_some() {
            return Some(self.full_expr.get().as_ref().unwrap());
        }
        if let Some(face) = self.inface.get() {
            if let Some(wire_expr) = self.wire_expr() {
                let wire_expr = wire_expr.to_owned();
                if self.prefix.get().is_none() {
                    if let Some(prefix) = zread!(face.tables.tables)
                        .get_mapping(&face.state, &wire_expr.scope, wire_expr.mapping)
                        .cloned()
                    {
                        let _ = self.prefix.set(prefix);
                    }
                }
                if let Some(prefix) = self.prefix.get().cloned() {
                    let _ = self
                        .full_expr
                        .set(prefix.expr() + wire_expr.suffix.as_ref());
                    return Some(self.full_expr.get().as_ref().unwrap());
                }
            }
        }
        None
    }
}
