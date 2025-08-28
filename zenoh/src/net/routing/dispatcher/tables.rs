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
    collections::HashMap,
    fmt::Debug,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
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
        dispatcher::{face::FaceId, gateway::BoundMap},
        hat::HatTrait,
        interceptor::{interceptor_factories, InterceptorFactory},
    },
    runtime::WeakRuntime,
};

pub(crate) struct RoutingExpr<'a> {
    pub(crate) prefix: &'a Arc<Resource>,
    pub(crate) suffix: &'a str,
    full: Option<String>,
}

impl Debug for RoutingExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}{}", self.prefix, self.suffix)
    }
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

pub(crate) struct TablesData {
    pub(crate) zid: ZenohIdProto,
    pub(crate) runtime: Option<WeakRuntime>,
    #[allow(dead_code)]
    pub(crate) hlc: Option<Arc<HLC>>,

    pub(crate) drop_future_timestamp: bool,
    pub(crate) queries_default_timeout: Duration,
    pub(crate) interests_timeout: Duration,

    pub(crate) root_res: Arc<Resource>,

    pub(crate) face_counter: FaceId,

    pub(crate) next_interceptor_version: AtomicUsize,
    pub(crate) interceptors: Vec<InterceptorFactory>,

    pub(crate) hats: BoundMap<HatTablesData>,
}

impl Debug for TablesData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TablesData").finish()
    }
}

pub(crate) struct HatTablesData {
    pub(crate) whatami: WhatAmI,
    pub(crate) faces: HashMap<FaceId, Arc<FaceState>>, // REVIEW(regions): move under TablesData?
    pub(crate) mcast_groups: Vec<Arc<FaceState>>,
    pub(crate) mcast_faces: Vec<Arc<FaceState>>,
    pub(crate) routes_version: RoutesVersion,
}

impl HatTablesData {
    pub(crate) fn new(whatami: WhatAmI) -> Self {
        HatTablesData {
            whatami,
            faces: HashMap::new(),
            mcast_groups: vec![],
            mcast_faces: vec![],
            routes_version: 0,
        }
    }
}

impl TablesData {
    pub fn new(
        zid: ZenohIdProto,
        hlc: Option<Arc<HLC>>,
        config: &Config,
        hat: BoundMap<HatTablesData>,
    ) -> ZResult<Self> {
        let drop_future_timestamp =
            unwrap_or_default!(config.timestamping().drop_future_timestamp());
        let queries_default_timeout =
            Duration::from_millis(unwrap_or_default!(config.queries_default_timeout()));
        let interests_timeout =
            Duration::from_millis(unwrap_or_default!(config.routing().interests().timeout()));
        Ok(TablesData {
            zid,
            runtime: None,
            hlc,
            drop_future_timestamp,
            queries_default_timeout,
            interests_timeout,
            root_res: Resource::root(),
            interceptors: interceptor_factories(config)?,
            next_interceptor_version: AtomicUsize::new(0),
            hats: hat,
            face_counter: 0,
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

    pub(crate) fn disable_all_routes(&mut self) {
        // REVIEW(regions): should hats invalidate each other's routes?
        for hat in self.hats.values_mut() {
            hat.routes_version = hat.routes_version.saturating_add(1);
        }
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub(crate) ctrl_lock: Mutex<()>,
    pub(crate) queries_lock: RwLock<()>,
}

pub struct Tables {
    pub data: TablesData,
    pub hats: BoundMap<Box<dyn HatTrait + Send + Sync>>,
}

impl Debug for Tables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tables").finish()
    }
}

impl TablesLock {
    #[allow(dead_code)]
    pub(crate) fn regen_interceptors(&self, config: &Config) -> ZResult<()> {
        let mut tables = zwrite!(self.tables);
        tables.data.interceptors = interceptor_factories(config)?;
        drop(tables);
        let tables = zread!(self.tables);
        let version = tables
            .data
            .next_interceptor_version
            .fetch_add(1, Ordering::SeqCst);

        for face in tables
            .data
            .hats
            .iter()
            .flat_map(|(_, hat)| hat.faces.values())
        {
            face.set_interceptors_from_factories(&tables.data.interceptors, version + 1)
        }
        Ok(())
    }
}
