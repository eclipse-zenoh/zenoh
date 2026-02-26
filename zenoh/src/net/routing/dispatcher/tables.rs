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
    borrow::Cow,
    cell::OnceCell,
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
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::{ExprId, WireExpr, ZenohIdProto},
    network::Mapping,
};
use zenoh_result::ZResult;

use super::face::FaceState;
pub use super::resource::*;
use crate::net::{
    routing::{
        dispatcher::{face::FaceId, region::RegionMap},
        hat::{HatTrait, Sources},
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

impl Debug for RoutingExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}{}", self.prefix, self.suffix)
    }
}

impl<'a> RoutingExpr<'a> {
    #[inline]
    pub(crate) fn new(prefix: &'a Arc<Resource>, suffix: &'a str) -> Self {
        let resource = if suffix.is_empty() {
            Some(prefix).into()
        } else {
            OnceCell::new()
        };
        RoutingExpr {
            prefix,
            suffix,
            resource,
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

    #[cfg(feature = "stats")]
    pub(crate) fn is_admin(&self) -> bool {
        let admin_prefix = "@/";
        if self.prefix.parent.is_none() {
            self.suffix.starts_with(admin_prefix)
        } else {
            self.prefix.expr().starts_with(admin_prefix)
        }
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

    pub(crate) faces: HashMap<FaceId, Arc<FaceState>>,

    #[cfg(feature = "stats")]
    pub(crate) stats: zenoh_stats::StatsRegistry,
    #[cfg(feature = "stats")]
    pub(crate) stats_keys: zenoh_stats::StatsKeysTree,

    pub(crate) hats: RegionMap<HatTablesData>,
}

impl Debug for TablesData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TablesData").finish()
    }
}

pub(crate) struct HatTablesData {
    pub(crate) mcast_groups: Vec<Arc<FaceState>>,
    // FIXME(regions): this field is apparently of no use (╯°□°)╯︵ ┻━┻
    pub(crate) mcast_faces: Vec<Arc<FaceState>>,
    pub(crate) routes_version: RoutesVersion,
}

impl HatTablesData {
    pub(crate) fn new() -> Self {
        HatTablesData {
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
        hat: RegionMap<HatTablesData>,
        #[cfg(feature = "stats")] stats: zenoh_stats::StatsRegistry,
    ) -> ZResult<Self> {
        let drop_future_timestamp =
            unwrap_or_default!(config.timestamping().drop_future_timestamp());
        let queries_default_timeout =
            Duration::from_millis(unwrap_or_default!(config.queries_default_timeout()));
        let interests_timeout =
            Duration::from_millis(unwrap_or_default!(config.routing().interests().timeout()));
        #[cfg(feature = "stats")]
        let mut stats_keys = zenoh_stats::StatsKeysTree::default();
        #[cfg(feature = "stats")]
        stats.update_keys(
            &mut stats_keys,
            config.stats.filters().iter().map(|f| &*f.key),
        );

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
            faces: HashMap::default(),
            #[cfg(feature = "stats")]
            stats_keys,
            #[cfg(feature = "stats")]
            stats,
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

    pub(crate) fn new_face_id(&mut self) -> FaceId {
        let face_id = self.face_counter;
        self.face_counter += 1;
        face_id
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub(crate) ctrl_lock: Mutex<()>,
    pub(crate) queries_lock: RwLock<()>,
    pub(crate) zid: ZenohIdProto, // FIXME(regions): refactor/remove
}

pub struct Tables {
    pub data: TablesData,
    pub hats: RegionMap<Box<dyn HatTrait + Send + Sync>>,
}

impl Tables {
    pub(crate) fn sourced_publishers(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| hat.sourced_publishers(&self.data))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }

    pub(crate) fn sourced_subscribers(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| hat.sourced_subscribers(&self.data))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }

    pub(crate) fn sourced_queryables(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| hat.sourced_queryables(&self.data))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }

    pub(crate) fn sourced_queriers(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| hat.sourced_queriers(&self.data))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }

    pub(crate) fn sourced_tokens(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| hat.sourced_tokens(&self.data))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }
}

impl Debug for Tables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tables").finish()
    }
}

impl TablesLock {
    #[allow(dead_code)]
    pub(crate) fn update_config(&self, config: &Config) -> ZResult<()> {
        let mut tables = zwrite!(self.tables);
        #[cfg(feature = "stats")]
        {
            let tables = &mut *tables;
            tables.data.stats.update_keys(
                &mut tables.data.stats_keys,
                config.stats.filters().iter().map(|k| &*k.key),
            );
        }
        tables.data.interceptors = interceptor_factories(config)?;
        drop(tables);
        let tables = zread!(self.tables);
        let version = tables
            .data
            .next_interceptor_version
            .fetch_add(1, Ordering::SeqCst);
        tables.data.faces.values().for_each(|face| {
            face.set_interceptors_from_factories(&tables.data.interceptors, version + 1);
        });
        Ok(())
    }
}
