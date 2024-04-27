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
use super::face::FaceState;
pub(in crate::sealed) use super::pubsub::*;
pub(in crate::sealed) use super::queries::*;
pub(in crate::sealed) use super::resource::*;
use crate::sealed::net::routing::hat;
use crate::sealed::net::routing::hat::HatTrait;
use crate::sealed::net::routing::interceptor::interceptor_factories;
use crate::sealed::net::routing::interceptor::InterceptorFactory;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::sync::{Mutex, RwLock};
use std::time::Duration;
use uhlc::HLC;
use zenoh_config::unwrap_or_default;
use zenoh_config::Config;
use zenoh_protocol::core::{ExprId, WhatAmI, ZenohId};
use zenoh_protocol::network::Mapping;
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;

pub(in crate::sealed) struct RoutingExpr<'a> {
    pub(in crate::sealed) prefix: &'a Arc<Resource>,
    pub(in crate::sealed) suffix: &'a str,
    full: Option<String>,
}

impl<'a> RoutingExpr<'a> {
    #[inline]
    pub(in crate::sealed) fn new(prefix: &'a Arc<Resource>, suffix: &'a str) -> Self {
        RoutingExpr {
            prefix,
            suffix,
            full: None,
        }
    }

    #[inline]
    pub(in crate::sealed) fn full_expr(&mut self) -> &str {
        if self.full.is_none() {
            self.full = Some(self.prefix.expr() + self.suffix);
        }
        self.full.as_ref().unwrap()
    }
}

pub(in crate::sealed) struct Tables {
    pub(in crate::sealed) zid: ZenohId,
    pub(in crate::sealed) whatami: WhatAmI,
    pub(in crate::sealed) face_counter: usize,
    #[allow(dead_code)]
    pub(in crate::sealed) hlc: Option<Arc<HLC>>,
    pub(in crate::sealed) drop_future_timestamp: bool,
    pub(in crate::sealed) queries_default_timeout: Duration,
    pub(in crate::sealed) root_res: Arc<Resource>,
    pub(in crate::sealed) faces: HashMap<usize, Arc<FaceState>>,
    pub(in crate::sealed) mcast_groups: Vec<Arc<FaceState>>,
    pub(in crate::sealed) mcast_faces: Vec<Arc<FaceState>>,
    pub(in crate::sealed) interceptors: Vec<InterceptorFactory>,
    pub(in crate::sealed) hat: Box<dyn Any + Send + Sync>,
    pub(in crate::sealed) hat_code: Arc<dyn HatTrait + Send + Sync>, // @TODO make this a Box
}

impl Tables {
    pub(in crate::sealed) fn new(
        zid: ZenohId,
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
        let hat_code = hat::new_hat(whatami, config);
        Ok(Tables {
            zid,
            whatami,
            face_counter: 0,
            hlc,
            drop_future_timestamp,
            queries_default_timeout,
            root_res: Resource::root(),
            faces: HashMap::new(),
            mcast_groups: vec![],
            mcast_faces: vec![],
            interceptors: interceptor_factories(config)?,
            hat: hat_code.new_tables(router_peers_failover_brokering),
            hat_code: hat_code.into(),
        })
    }

    #[doc(hidden)]
    pub(in crate::sealed) fn _get_root(&self) -> &Arc<Resource> {
        &self.root_res
    }

    #[cfg(test)]
    pub(in crate::sealed) fn print(&self) -> String {
        Resource::print_tree(&self.root_res)
    }

    #[inline]
    pub(in crate::sealed) fn get_mapping<'a>(
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
    pub(in crate::sealed) fn get_sent_mapping<'a>(
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
    pub(in crate::sealed) fn get_face(&self, zid: &ZenohId) -> Option<&Arc<FaceState>> {
        self.faces.values().find(|face| face.zid == *zid)
    }

    fn update_routes(&mut self, res: &mut Arc<Resource>) {
        update_data_routes(self, res);
        update_query_routes(self, res);
    }

    pub(in crate::sealed) fn update_matches_routes(&mut self, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            self.update_routes(res);

            let resclone = res.clone();
            for match_ in &mut get_mut_unchecked(res).context_mut().matches {
                let match_ = &mut match_.upgrade().unwrap();
                if !Arc::ptr_eq(match_, &resclone) && match_.context.is_some() {
                    self.update_routes(match_);
                }
            }
        }
    }
}

pub(in crate::sealed) fn close_face(tables: &TablesLock, face: &Weak<FaceState>) {
    match face.upgrade() {
        Some(mut face) => {
            tracing::debug!("Close {}", face);
            face.task_controller.terminate_all(Duration::from_secs(10));
            finalize_pending_queries(tables, &mut face);
            zlock!(tables.ctrl_lock).close_face(tables, &mut face);
        }
        None => tracing::error!("Face already closed!"),
    }
}

pub(in crate::sealed) struct TablesLock {
    pub(in crate::sealed) tables: RwLock<Tables>,
    pub(in crate::sealed) ctrl_lock: Mutex<Box<dyn HatTrait + Send + Sync>>,
    pub(in crate::sealed) queries_lock: RwLock<()>,
}
