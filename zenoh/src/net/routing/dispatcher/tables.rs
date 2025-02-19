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
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use uhlc::HLC;
use zenoh_config::{unwrap_or_default, Config};
use zenoh_protocol::{
    core::{ExprId, WhatAmI, ZenohIdProto},
    network::Mapping,
};
use zenoh_result::ZResult;

use super::face::FaceState;
pub use super::resource::*;
use crate::net::{
    routing::{
        hat::{self, HatTrait},
        interceptor::{interceptor_factories, InterceptorFactory},
    },
    runtime::WeakRuntime,
};

pub(crate) struct RoutingExpr<'a> {
    pub(crate) prefix: &'a Arc<Resource>,
    pub(crate) suffix: &'a str,
    full: Option<String>,
}

impl<'a> RoutingExpr<'a> {
    #[inline]
    pub(crate) fn new(prefix: &'a Arc<Resource>, suffix: &'a str) -> Self {
        RoutingExpr {
            prefix,
            suffix,
            full: None,
        }
    }

    #[inline]
    pub(crate) fn full_expr(&mut self) -> &str {
        if self.full.is_none() {
            self.full = Some(self.prefix.expr().to_string() + self.suffix);
        }
        self.full.as_ref().unwrap()
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
    pub(crate) hat_code: Arc<dyn HatTrait + Send + Sync>, // @TODO make this a Box
    pub(crate) routes_version: RoutesVersion,
}

impl Tables {
    pub fn new(
        zid: ZenohIdProto,
        whatami: WhatAmI,
        hlc: Option<Arc<HLC>>,
        config: &Config,
    ) -> ZResult<Self> {
        let drop_future_timestamp =
            unwrap_or_default!(config.timestamping().drop_future_timestamp());
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        let queries_default_timeout =
            Duration::from_millis(unwrap_or_default!(config.queries_default_timeout()));
        let interests_timeout =
            Duration::from_millis(unwrap_or_default!(config.routing().interests().timeout()));
        let hat_code = hat::new_hat(whatami, config);
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
            hat_code: hat_code.into(),
            routes_version: 0,
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

    #[allow(dead_code)]
    pub(crate) fn regen_interceptors(&self) {
        self.faces
            .values()
            .for_each(|face| face.regen_interceptors(&self.interceptors));
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub(crate) ctrl_lock: Mutex<Box<dyn HatTrait + Send + Sync>>,
    pub queries_lock: RwLock<()>,
}
