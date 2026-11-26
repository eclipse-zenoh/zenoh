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
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_protocol::{
    core::{Bound, ExprId, Region, WireExpr, ZenohIdProto},
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
    pub(crate) routes_version: RoutesVersion,

    /// Northbound declaration aggregation (`aggregation.upstream`). Prefixes whose downstream
    /// subscribers/queryables a north-bound router HAT folds into one `${prefix}` declaration
    /// upstream. `*_resources` are the corresponding aggregate `Resource`s, pre-created once at
    /// gateway build (parallel to the prefix vecs) so the fold path never creates resources and the
    /// aggregate's match-set is wired exactly once. Empty ⇒ behaviour identical to stock zenoh.
    pub(crate) agg_sub_prefixes: Vec<OwnedKeyExpr>,
    pub(crate) agg_sub_resources: Vec<Arc<Resource>>,
    pub(crate) agg_qabl_prefixes: Vec<OwnedKeyExpr>,
    pub(crate) agg_qabl_resources: Vec<Arc<Resource>>,
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
        let agg_sub_prefixes = config.aggregation().upstream().subscribers().clone();
        let agg_qabl_prefixes = config.aggregation().upstream().queryables().clone();
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
            routes_version: 0,
            agg_sub_prefixes,
            agg_sub_resources: Vec::new(),
            agg_qabl_prefixes,
            agg_qabl_resources: Vec::new(),
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

    pub(crate) fn new_face_id(&mut self) -> FaceId {
        let face_id = self.face_counter;
        self.face_counter += 1;
        face_id
    }

    /// Disable all hats' data and query routes **for all resources**.
    pub(crate) fn disable_all_routes(&mut self) {
        let routes_version = &mut self.routes_version;
        *routes_version = routes_version.saturating_add(1);
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub(crate) ctrl_lock: Mutex<()>,
    pub(crate) queries_lock: RwLock<()>,
}

/// Gateway state.
///
/// In order to mutably borrow [`Tables::data`] and [`Tables::hats`] at the same time given an
/// acquired guard from [`TablesLock::tables`], one needs to first dereference the guard into [`Tables`]:
///
/// ```rust,ignore
/// fn borrow_hats_and_data(tables_lock: &TablesLock) {
///     let mut guard = tables_lock.tables.write().unwrap();
///
///     // NOTE(regions): The following doesn't compile due to E0499:
///     // let hats = &mut guard.hats;
///     // let tables_data = &mut guard.data;
///     // drop((hats, tables_data));
///
///     let tables = &mut *guard;
///     let hats = &mut tables.hats;
///     let tables_data = &mut tables.data;
///     drop((hats, tables_data));
/// }
/// ```
pub struct Tables {
    pub data: TablesData,
    pub hats: RegionMap<Box<dyn HatTrait + Send + Sync>>,
}

impl Tables {
    /// Pre-create the aggregate `Resource` for every configured `aggregation.upstream` prefix, once,
    /// at gateway build (where the full `Tables` is available for `make_resource`). The northbound
    /// fold path then only *looks these up* — it never creates resources — and because each
    /// aggregate persists for the gateway's lifetime its match-set is wired exactly once, so
    /// reconnect/churn cannot accumulate duplicate match cross-links. No-op when unconfigured.
    pub(crate) fn precreate_upstream_aggregates(&mut self) {
        self.warn_on_aggregation_misconfig();
        // Create + wire each UNIQUE prefix's aggregate exactly once (a prefix may be listed in both
        // `subscribers` and `queryables`), then fill the parallel resource vecs by lookup. This
        // avoids re-running `match_resource` on a shared prefix (which would duplicate match links).
        let mut by_ke: HashMap<OwnedKeyExpr, Arc<Resource>> = HashMap::new();
        let prefixes: Vec<OwnedKeyExpr> = self
            .data
            .agg_sub_prefixes
            .iter()
            .chain(self.data.agg_qabl_prefixes.iter())
            .cloned()
            .collect();
        for ke in &prefixes {
            if !by_ke.contains_key(ke) {
                let res = self.make_aggregate_resource(ke);
                by_ke.insert(ke.clone(), res);
            }
        }
        let sub_res: Vec<Arc<Resource>> = self
            .data
            .agg_sub_prefixes
            .iter()
            .map(|ke| by_ke[ke].clone())
            .collect();
        let qabl_res: Vec<Arc<Resource>> = self
            .data
            .agg_qabl_prefixes
            .iter()
            .map(|ke| by_ke[ke].clone())
            .collect();
        self.data.agg_sub_resources = sub_res;
        self.data.agg_qabl_resources = qabl_res;
    }

    /// Emit operator-facing warnings for suspicious `aggregation.upstream` prefixes (root wildcards,
    /// admin-space prefixes, duplicates, mutual inclusion) plus a one-time note about the trade-offs.
    /// Soft: never rejects config — folding still applies to the prefixes exactly as given.
    fn warn_on_aggregation_misconfig(&self) {
        let all: Vec<(&str, &OwnedKeyExpr)> = self
            .data
            .agg_sub_prefixes
            .iter()
            .map(|k| ("subscribers", k))
            .chain(
                self.data
                    .agg_qabl_prefixes
                    .iter()
                    .map(|k| ("queryables", k)),
            )
            .collect();
        if all.is_empty() {
            return;
        }
        tracing::info!(
            "northbound aggregation enabled for {} prefix declaration(s): upstream per-key declaration \
             ACL/QoS/interceptors and upstream admin-space enumeration see only the ${{prefix}} aggregate, \
             and the wildcard aggregate may forward data toward the edge for unsubscribed keys — apply \
             per-key policy and per-key introspection at the owning edge",
            all.len()
        );
        for (kind, ke) in &all {
            let s = ke.as_str();
            if s == "**" || s == "*" {
                tracing::warn!(
                    "aggregation.upstream.{kind} prefix `{s}` is a root wildcard and would fold EVERY \
                     matching declaration into one aggregate — almost certainly a misconfiguration"
                );
            }
            if s == "@" || s.starts_with("@/") {
                tracing::warn!(
                    "aggregation.upstream.{kind} prefix `{s}` targets the `@/` admin key-space; folding \
                     admin declarations breaks per-node introspection — almost certainly a misconfiguration"
                );
            }
        }
        // Duplicates / mutual inclusion: matching is first-match, so a narrower prefix listed after a
        // broader one that includes it never receives children (its aggregate stays empty).
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let (a, b) = (all[i].1, all[j].1);
                if a == b {
                    tracing::warn!(
                        "aggregation.upstream prefix `{}` is listed more than once (redundant)",
                        a.as_str()
                    );
                } else if a.includes(b) {
                    tracing::warn!(
                        "aggregation.upstream prefix `{}` includes `{}`: children of the narrower prefix \
                         fold into the broader one (first match), so `{}` may never aggregate anything",
                        a.as_str(),
                        b.as_str(),
                        b.as_str()
                    );
                } else if b.includes(a) {
                    tracing::warn!(
                        "aggregation.upstream prefix `{}` includes `{}`: children of the narrower prefix \
                         fold into the broader one (first match), so `{}` may never aggregate anything",
                        b.as_str(),
                        a.as_str(),
                        a.as_str()
                    );
                }
            }
        }
    }

    fn make_aggregate_resource(&mut self, ke: &OwnedKeyExpr) -> Arc<Resource> {
        // Compute matches BEFORE creating the resource so it does not appear in its own match-set
        // (mirrors `Face::with_mapped_expr`), then wire it in so a wildcard route resolving to the
        // aggregate still reaches the (later-declared) children.
        let mut matches = Resource::get_matches(&self.data, ke);
        let mut root = self.data.root_res.clone();
        let mut res = Resource::make_resource(self, &mut root, ke.as_str());
        matches.push(Arc::downgrade(&res));
        Resource::match_resource(&self.data, &mut res, matches);
        res
    }

    pub(crate) fn sourced_subscribers(&self) -> HashMap<Arc<Resource>, Sources> {
        self.hats
            .values()
            .flat_map(|hat| {
                self.add_north_source(&hat.region(), hat.sourced_subscribers(&self.data))
            })
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
            .flat_map(|hat| {
                self.add_north_source(&hat.region(), hat.sourced_queryables(&self.data))
            })
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
            .flat_map(|hat| self.add_north_source(&hat.region(), hat.sourced_tokens(&self.data)))
            .fold(HashMap::new(), |mut acc, (res, src)| {
                acc.entry(res.clone())
                    .and_modify(|s| s.extend(&src))
                    .or_insert(src);
                acc
            })
    }

    /// For entities sourced from a south region, adds the gateway's own ZID as a north source so
    /// they are visible to north-facing peers. Note that hats only return **remote** entities.
    fn add_north_source(
        &self,
        region: &Region,
        mut entities: HashMap<Arc<Resource>, Sources>,
    ) -> HashMap<Arc<Resource>, Sources> {
        if region.bound().is_south() {
            let north_source =
                Sources::empty().with_mode([self.data.zid], self.hats[Region::North].mode());
            for entity in entities.values_mut().filter(|e| !e.is_empty()) {
                entity.extend(&north_source);
            }
        }

        entities
    }

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

    #[inline]
    pub(crate) fn ingress_filter(&self, _src_face: &FaceState) -> bool {
        true
    }

    #[inline]
    pub(crate) fn egress_filter(&self, src_face: &FaceState, out_face: &Arc<FaceState>) -> bool {
        src_face.id != out_face.id
            && (out_face.mcast_group.is_none() || src_face.mcast_group.is_none())
    }
}

/// Inter-region filter.
///
/// We reject messages in two scenarios:
/// 1. The source node is a gateway.
/// 2. We are not the primary gateway.
///
/// # South-to-North
///
/// ```text
///   fwd_zid = Some(F)                        fwd_zid = None
///   ┌───────────────────┐                    ┌───────────────────┐
///   │                   │                    │                   │
///   │     G   ...  G'   │                    │     G  ...  G'    │
///   └─────/─────────\───┘                    └─────/─────────\───┘
///   ┌────/───────────\──┐                    ┌────/───────────\──┐
///   │   G     ...    G' │                    │   G     ...    G' │
///   │    \          /   │                    │                   │
///   │     F ─ ─ ─ ─     │  fwd (south)       │     F             │  fwd (south)
///   │     |             │                    │                   │
///   │     S             │  src (south)       │     S             │  src (south)
///   └───────────────────┘                    └───────────────────┘
///
///   gwys = gateways known to F                gwys = all region gateways
/// ```
///
/// # North-to-South
///
/// ```text
///   dst_zid = Some(D)                        dst_zid = None
///   ┌───────────────────┐                    ┌───────────────────┐
///   │       S           │  src               │       S           │  src
///   │                   │                    │                   │
///   │     G  ...  G'    │                    │     G   ...   G'  │
///   └─────/─────────\───┘                    └─────/─────────\───┘
///   ┌────/───────────\──┐                    ┌────/───────────\──┐
///   │   G0    ...    G1 │                    │   G     ...    G' │
///   │    \          /   │                    │                   │
///   │     D ─ ─ ─ ─     │  dst (south)       │     D             │ dst (south)
///   └───────────────────┘                    └───────────────────┘
///
///   gwys = gateways known to D                gwys = all region gateways
/// ```
///
/// In both cases: `primary = max(gwys)`, allow iff `self.zid == primary`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InterRegionFilter<'a> {
    /// Source region.
    pub src: &'a Region,
    /// Destination region.
    pub dst: &'a Region,
    /// ZID of the source node which is potentially different than [`InterRegionFilter::fwd_zid`] in
    /// linkstate.
    pub src_zid: Option<&'a ZenohIdProto>,
    /// ZID of the source face.
    pub fwd_zid: Option<&'a ZenohIdProto>,
    /// ZID of the destination node.
    pub dst_zid: Option<&'a ZenohIdProto>,
}

impl InterRegionFilter<'_> {
    /// Returns `false` if a `Push` or `Request` should be filtered out.
    pub(crate) fn resolve(&self, tables: &Tables) -> bool {
        // NOTE(regions): we initialize the tracing span iff the TRACE level is enabled and we use
        // aux to trace the filtering event.

        let _span = tracing::enabled!(tracing::Level::TRACE).then(|| {
            tracing::trace_span!(
                "inter_region_filter",
                src = %self.src,
                dst = %self.dst,
                src_zid = ?self.src_zid.map(|zid| zid.short()),
                fwd_zid = ?self.fwd_zid.map(|zid| zid.short()),
                dst_zid = ?self.dst_zid.map(|zid| zid.short()),
            )
            .entered()
        });

        let ret = aux(self, tables);
        tracing::trace!(return = ret);
        return ret;

        fn aux(this: &InterRegionFilter<'_>, tables: &Tables) -> bool {
            if this.src.bound() == this.dst.bound() {
                // NOTE(regions): only messages traveling downstream/upstream need filtering, i.e. in
                // the presence of multiple region gateways.
                return true;
            }

            let gwys = match this.src.bound() {
                Bound::North => this
                    .dst_zid
                    .map(|dst_zid| tables.hats[this.dst].gateways_of(&tables.data, dst_zid))
                    .unwrap_or_else(|| tables.hats[this.dst].gateways(&tables.data)),
                // NOTE(regions): in this case, we cannot have a link with `src_zid` if it were also a
                // gateway so we check the "forwarder's" zid. This works for routers and peers alike.
                // See `multiple_gateways_data_routing_r2r_upstream_gateway_source` in
                // net::tests::regions::routing.
                Bound::South => this
                    .fwd_zid
                    .map(|fwd_zid| tables.hats[this.src].gateways_of(&tables.data, fwd_zid))
                    .unwrap_or_else(|| tables.hats[this.src].gateways(&tables.data)),
            };

            let Some(gwys) = gwys.filter(|g| !g.is_empty()) else {
                return true;
            };

            let Some(src_zid) = this.src_zid else {
                bug!("Unknown source ZID");
                return true;
            };

            // HACK(regions): this has the effect of filtering out messages originating from a router
            // subregion from being re-routed to the north (router) region. As the router linkstate
            // protocol is unimplemented, our hand is forced.
            if gwys.contains(src_zid) {
                return false;
            }

            // SAFETY: gwys is guaranteed non-empty by the filter above.
            let primary = gwys.iter().max().unwrap();

            &tables.data.zid == primary
        }
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
