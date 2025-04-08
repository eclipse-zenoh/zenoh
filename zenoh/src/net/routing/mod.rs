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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
pub mod dispatcher;
pub mod hat;
pub mod interceptor;
pub mod namespace;
pub mod router;

use std::{cell::OnceCell, sync::Arc};

use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::WireExpr,
    network::{NetworkMessageExt, NetworkMessageMut},
};

use self::{dispatcher::face::Face, router::Resource};
use super::runtime;

pub(crate) struct RoutingContext<Msg> {
    pub(crate) msg: Msg,
    pub(crate) inface: OnceCell<Face>,
    pub(crate) outface: OnceCell<Face>,
    pub(crate) prefix: OnceCell<Arc<Resource>>,
    pub(crate) full_expr: OnceCell<String>,
}

impl<Msg> RoutingContext<Msg> {
    #[allow(dead_code)]
    pub(crate) fn new(msg: Msg) -> Self {
        Self {
            msg,
            inface: OnceCell::new(),
            outface: OnceCell::new(),
            prefix: OnceCell::new(),
            full_expr: OnceCell::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new_in(msg: Msg, inface: Face) -> Self {
        Self {
            msg,
            inface: OnceCell::from(inface),
            outface: OnceCell::new(),
            prefix: OnceCell::new(),
            full_expr: OnceCell::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new_out(msg: Msg, outface: Face) -> Self {
        Self {
            msg,
            inface: OnceCell::new(),
            outface: OnceCell::from(outface),
            prefix: OnceCell::new(),
            full_expr: OnceCell::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_expr(msg: Msg, expr: String) -> Self {
        Self {
            msg,
            inface: OnceCell::new(),
            outface: OnceCell::new(),
            prefix: OnceCell::new(),
            full_expr: OnceCell::from(expr),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn inface(&self) -> Option<&Face> {
        self.inface.get()
    }

    #[allow(dead_code)]
    pub(crate) fn outface(&self) -> Option<&Face> {
        self.outface.get()
    }

    pub(crate) fn with_mut<R>(mut self, f: impl FnOnce(RoutingContext<&mut Msg>) -> R) -> R {
        f(RoutingContext {
            msg: &mut self.msg,
            inface: self.inface,
            outface: self.outface,
            prefix: self.prefix,
            full_expr: self.full_expr,
        })
    }
}

impl RoutingContext<NetworkMessageMut<'_>> {
    #[inline]
    pub(crate) fn wire_expr(&self) -> Option<&WireExpr> {
        self.msg.wire_expr()
    }

    #[inline]
    pub(crate) fn prefix(&self) -> Option<&Arc<Resource>> {
        if let Some(face) = self.outface.get() {
            if let Some(wire_expr) = self.wire_expr() {
                let wire_expr = wire_expr.to_owned();
                if self.prefix.get().is_none() {
                    if let Some(prefix) = zread!(face.tables.tables)
                        .get_sent_mapping(&face.state, &wire_expr.scope, wire_expr.mapping)
                        .cloned()
                    {
                        let _ = self.prefix.set(prefix);
                    }
                }
                return self.prefix.get();
            }
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
                return self.prefix.get();
            }
        }
        None
    }

    #[inline]
    pub(crate) fn full_expr(&self) -> Option<&str> {
        if self.full_expr.get().is_some() {
            return Some(self.full_expr.get().as_ref().unwrap());
        }
        if let Some(prefix) = self.prefix() {
            let _ = self
                .full_expr
                .set(prefix.expr().to_string() + self.wire_expr().unwrap().suffix.as_ref());
            return Some(self.full_expr.get().as_ref().unwrap());
        }
        None
    }

    #[inline]
    pub(crate) fn full_keyexpr(&self) -> Option<&keyexpr> {
        let full_expr = self.full_expr()?;
        keyexpr::new(full_expr).ok()
    }
}
