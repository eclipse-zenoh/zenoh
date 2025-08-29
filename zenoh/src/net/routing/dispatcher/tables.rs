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
    borrow::Cow,
    cell::OnceCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use uhlc::HLC;
use zenoh_config::{unwrap_or_default, Config};
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::{ExprId, WhatAmI, WireExpr, ZenohIdProto},
    network::Mapping,
};
use zenoh_result::ZResult;

use super::face::FaceState;
pub use super::resource::*;
use crate::net::{
    routing::{
        hat::HatTrait,
        interceptor::{interceptor_factories, InterceptorFactory},
    },
    runtime::WeakRuntime,
};

pub(crate) struct RoutingExpr<'a> {
    prefix: &'a Arc<Resource>,
    suffix: &'a str,
    resource: OnceCell<Option<&'a Arc<Resource>>>,
    key_expr: OnceCell<Option<Cow<'a, keyexpr>>>,
}

impl<'a> RoutingExpr<'a> {
    #[inline]
    pub(crate) fn new(prefix: &'a Arc<Resource>, suffix: &'a str) -> Self {
        RoutingExpr {
            prefix,
            suffix,
            resource: OnceCell::new(),
            key_expr: OnceCell::new(),
        }
    }

    pub(crate) fn resource(&self) -> Option<&'a Arc<Resource>> {
        *self
            .resource
            .get_or_init(|| Resource::get_resource_ref(self.prefix, self.suffix))
    }

    fn compute_key_expr(&self) -> Option<Cow<'a, keyexpr>> {
        let full_expr = match self.resource().as_ref() {
            Some(res) => res
                .keyexpr()
                .ok_or_else(|| keyexpr::new("").unwrap_err())
                .map(Cow::Borrowed),
            None => [self.prefix.expr(), self.suffix]
                .concat()
                .try_into()
                .map(Cow::Owned),
        };
        if let Err(e) = &full_expr {
            tracing::warn!("Invalid KE reached the system: {}", e);
        }
        full_expr.ok()
    }

    pub(crate) fn key_expr(&self) -> Option<&keyexpr> {
        self.key_expr
            .get_or_init(|| self.compute_key_expr())
            .as_deref()
    }

    pub(crate) fn get_best_key(&self, sid: usize) -> WireExpr<'a> {
        match self.resource() {
            Some(res) => res.get_best_key("", sid),
            None => self.prefix.get_best_key(self.suffix, sid),
        }
    }
}

pub struct Tables {
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    pub(crate) runtime: Option<WeakRuntime>,
    pub(crate) face_counter: usize,
    #[allow(dead_code)]
    pub(crate) hlc: Option<Arc<HLC>>,
    pub(crate) drop_future_timestamp: bool,
    pub(crate) queries_default_timeout: Duration,
    pub(crate) interests_timeout: Duration,
    pub(crate) root_res: Arc<Resource>,
    pub(crate) faces: HashMap<usize, Arc<FaceState>>,
    pub(crate) mcast_groups: Vec<Arc<FaceState>>,
    pub(crate) mcast_faces: Vec<Arc<FaceState>>,
    pub(crate) interceptors: Vec<InterceptorFactory>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) routes_version: RoutesVersion,
    pub(crate) next_interceptor_version: AtomicUsize,
}

impl Tables {
    pub fn new(
        zid: ZenohIdProto,
        whatami: WhatAmI,
        hlc: Option<Arc<HLC>>,
        config: &Config,
        hat_code: &(dyn HatTrait + Send + Sync),
    ) -> ZResult<Self> {
        let drop_future_timestamp =
            unwrap_or_default!(config.timestamping().drop_future_timestamp());
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        let queries_default_timeout =
            Duration::from_millis(unwrap_or_default!(config.queries_default_timeout()));
        let interests_timeout =
            Duration::from_millis(unwrap_or_default!(config.routing().interests().timeout()));
        Ok(Tables {
            zid,
            whatami,
            runtime: None,
            face_counter: 0,
            hlc,
            drop_future_timestamp,
            queries_default_timeout,
            interests_timeout,
            root_res: Resource::root(),
            faces: HashMap::new(),
            mcast_groups: vec![],
            mcast_faces: vec![],
            interceptors: interceptor_factories(config)?,
            hat: hat_code.new_tables(router_peers_failover_brokering),
            routes_version: 0,
            next_interceptor_version: AtomicUsize::new(0),
        })
    }

    #[doc(hidden)]
    pub fn _get_root(&self) -> &Arc<Resource> {
        &self.root_res
    }

    #[cfg(test)]
    pub fn print(&self) -> String {
        Resource::print_tree(&self.root_res)
    }

    #[inline]
    pub(crate) fn get_mapping<'a>(
        &'a self,
        face: &'a FaceState,
        expr_id: &ExprId,
        mapping: Mapping,
    ) -> Option<&'a Arc<Resource>> {
        match expr_id {
            0 => Some(&self.root_res),
            expr_id => face.get_mapping(expr_id, mapping),
        }
    }

    #[inline]
    pub(crate) fn get_sent_mapping<'a>(
        &'a self,
        face: &'a FaceState,
        expr_id: &ExprId,
        mapping: Mapping,
    ) -> Option<&'a Arc<Resource>> {
        match expr_id {
            0 => Some(&self.root_res),
            expr_id => face.get_sent_mapping(expr_id, mapping),
        }
    }

    #[inline]
    pub(crate) fn get_face(&self, zid: &ZenohIdProto) -> Option<&Arc<FaceState>> {
        self.faces.values().find(|face| face.zid == *zid)
    }

    pub(crate) fn disable_all_routes(&mut self) {
        self.routes_version = self.routes_version.saturating_add(1);
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub(crate) hat_code: Box<dyn HatTrait + Send + Sync>,
    pub(crate) ctrl_lock: Mutex<()>,
    pub queries_lock: RwLock<()>,
}

impl TablesLock {
    #[allow(dead_code)]
    pub(crate) fn regen_interceptors(&self, config: &Config) -> ZResult<()> {
        let mut tables = zwrite!(self.tables);
        tables.interceptors = interceptor_factories(config)?;
        drop(tables);
        let tables = zread!(self.tables);
        let version = tables
            .next_interceptor_version
            .fetch_add(1, Ordering::SeqCst);
        tables.faces.values().for_each(|face| {
            face.set_interceptors_from_factories(&tables.interceptors, version + 1);
        });
        Ok(())
    }
}
