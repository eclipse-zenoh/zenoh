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

use std::{any::Any, cell::OnceCell};

use zenoh_protocol::network::NetworkMessageMut;

use super::runtime;
use crate::net::routing::{dispatcher::face::Face, interceptor::InterceptorContext};

pub(crate) struct RoutingContext<Msg> {
    pub(crate) msg: Msg,
    pub(crate) full_expr: OnceCell<String>,
}

impl<Msg> RoutingContext<Msg> {
    #[allow(dead_code)]
    pub(crate) fn new(msg: Msg) -> Self {
        Self {
            msg,
            full_expr: OnceCell::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_expr(msg: Msg, expr: String) -> Self {
        Self {
            msg,
            full_expr: OnceCell::from(expr),
        }
    }

    pub(crate) fn with_mut<R>(mut self, f: impl FnOnce(RoutingContext<&mut Msg>) -> R) -> R {
        f(RoutingContext {
            msg: &mut self.msg,
            full_expr: self.full_expr,
        })
    }
}

impl<T> InterceptorContext for RoutingContext<T> {
    fn face(&self) -> Option<Face> {
        None
    }
    fn full_expr(&self, _msg: &NetworkMessageMut) -> Option<&str> {
        self.full_expr.get().map(|x| x.as_str())
    }
    fn get_cache(&self, _msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
        None
    }
}
