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
    borrow::{Borrow, Cow},
    collections::VecDeque,
    convert::TryInto,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock, Weak},
};

pub(crate) mod resource_trace {
    use super::Resource;
    use std::{
        collections::{HashMap, VecDeque},
        fmt,
        io::{self, Write},
        panic,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc, Mutex, MutexGuard, OnceLock, Weak,
        },
    };

    #[derive(Clone, Debug, Default)]
    struct ResourceRecord {
        expr: String,
        parent: Option<usize>,
        created_seq: u64,
        context_seq: u64,
        dropped_seq: u64,
        last_seq: u64,
        last_op: String,
    }

    struct TraceState {
        enabled: bool,
        max_events: usize,
        seq: AtomicU64,
        dumped: AtomicBool,
        events: Mutex<VecDeque<String>>,
        registry: Mutex<HashMap<usize, ResourceRecord>>,
    }

    static STATE: OnceLock<TraceState> = OnceLock::new();

    fn init_state() -> TraceState {
        let enabled = std::env::var("ZENOH_RESOURCE_TRACE")
            .map(|v| !v.is_empty() && v != "0" && v.to_ascii_lowercase() != "false")
            .unwrap_or(false);
        let max_events = std::env::var("ZENOH_RESOURCE_TRACE_RING")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000);
        if enabled {
            let previous_hook = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                dump("panic hook");
                previous_hook(info);
            }));
        }
        TraceState {
            enabled,
            max_events,
            seq: AtomicU64::new(0),
            dumped: AtomicBool::new(false),
            events: Mutex::new(VecDeque::with_capacity(max_events.min(1024))),
            registry: Mutex::new(HashMap::new()),
        }
    }

    fn state() -> &'static TraceState {
        STATE.get_or_init(init_state)
    }

    fn lock<'a, T>(mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn enabled() -> bool {
        state().enabled
    }

    fn thread_label() -> String {
        let thread = std::thread::current();
        match thread.name() {
            Some(name) => format!("{:?}/{}", thread.id(), name),
            None => format!("{:?}", thread.id()),
        }
    }

    fn next_seq() -> u64 {
        state().seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn push_event(seq: u64, msg: String) {
        let state = state();
        let mut events = lock(&state.events);
        if events.len() >= state.max_events {
            events.pop_front();
        }
        events.push_back(format!(
            "seq={} pid={} thread={} {}",
            seq,
            std::process::id(),
            thread_label(),
            msg
        ));
    }

    pub(crate) fn event(args: fmt::Arguments<'_>) {
        if !enabled() {
            return;
        }
        let seq = next_seq();
        push_event(seq, args.to_string());
    }

    fn resource_ptr(res: &Resource) -> usize {
        res as *const Resource as usize
    }

    pub(crate) fn arc_ptr(res: &Arc<Resource>) -> usize {
        Arc::as_ptr(res) as usize
    }

    pub(crate) fn weak_ptr(weak: &Weak<Resource>) -> usize {
        Weak::as_ptr(weak) as usize
    }

    fn parent_ptr(res: &Resource) -> Option<usize> {
        res.parent.as_ref().map(arc_ptr)
    }

    fn upsert_record(ptr: usize, expr: String, parent: Option<usize>, seq: u64, op: &str) {
        let mut registry = lock(&state().registry);
        let record = registry.entry(ptr).or_insert_with(ResourceRecord::default);
        if !expr.is_empty() || record.expr.is_empty() {
            record.expr = expr;
        }
        record.parent = parent;
        record.last_seq = seq;
        record.last_op = op.to_string();
    }

    pub(crate) fn mark_created(res: &Arc<Resource>, op: &str) {
        if !enabled() {
            return;
        }
        let seq = next_seq();
        let ptr = arc_ptr(res);
        let parent = parent_ptr(res);
        {
            let mut registry = lock(&state().registry);
            let record = registry.entry(ptr).or_insert_with(ResourceRecord::default);
            record.expr = res.expr.clone();
            record.parent = parent;
            record.created_seq = seq;
            record.last_seq = seq;
            record.last_op = op.to_string();
        }
        push_event(seq, format!("RESOURCE_CREATED op={} {}", op, arc_summary(res)));
    }

    pub(crate) fn mark_context_attached(res: &Arc<Resource>, op: &str) {
        if !enabled() {
            return;
        }
        let seq = next_seq();
        let ptr = arc_ptr(res);
        let parent = parent_ptr(res);
        {
            let mut registry = lock(&state().registry);
            let record = registry.entry(ptr).or_insert_with(ResourceRecord::default);
            record.expr = res.expr.clone();
            record.parent = parent;
            record.context_seq = seq;
            record.last_seq = seq;
            record.last_op = op.to_string();
        }
        push_event(seq, format!("RESOURCE_CONTEXT_ATTACHED op={} {}", op, arc_summary(res)));
    }

    pub(crate) fn mark_drop(res: &Resource) {
        if !enabled() {
            return;
        }
        let seq = next_seq();
        let ptr = resource_ptr(res);
        {
            let mut registry = lock(&state().registry);
            let record = registry.entry(ptr).or_insert_with(ResourceRecord::default);
            record.expr = res.expr.clone();
            record.parent = parent_ptr(res);
            record.dropped_seq = seq;
            record.last_seq = seq;
            record.last_op = "drop".to_string();
        }
        push_event(
            seq,
            format!(
                "RESOURCE_DROP ptr=0x{ptr:x} expr={:?} parent={} children={} context={} matches={} sessions={}",
                res.expr,
                parent_ptr(res)
                    .map(|p| format!("0x{p:x}"))
                    .unwrap_or_else(|| "none".to_string()),
                res.children.iter().count(),
                res.context.is_some(),
                res.context.as_ref().map(|c| c.matches.len()).unwrap_or(0),
                res.session_ctxs.len(),
            ),
        );
    }

    pub(crate) fn mark_resource_event(op: &str, res: &Arc<Resource>) {
        if !enabled() {
            return;
        }
        let seq = next_seq();
        upsert_record(arc_ptr(res), res.expr.clone(), parent_ptr(res), seq, op);
        push_event(seq, format!("{} {}", op, arc_summary(res)));
    }

    pub(crate) fn arc_summary(res: &Arc<Resource>) -> String {
        format!(
            "ptr=0x{:x} expr={:?} parent={} strong={} weak={} children={} context={} matches={} sessions={}",
            arc_ptr(res),
            res.expr,
            parent_ptr(res)
                .map(|p| format!("0x{p:x}"))
                .unwrap_or_else(|| "none".to_string()),
            Arc::strong_count(res),
            Arc::weak_count(res),
            res.children.iter().count(),
            res.context.is_some(),
            res.context.as_ref().map(|c| c.matches.len()).unwrap_or(0),
            res.session_ctxs.len(),
        )
    }

    pub(crate) fn weak_summary(weak: &Weak<Resource>) -> String {
        let ptr = weak_ptr(weak);
        match weak.upgrade() {
            Some(res) => format!("weak=0x{ptr:x} live=true {}", arc_summary(&res)),
            None => {
                let registry = lock(&state().registry);
                match registry.get(&ptr) {
                    Some(record) => format!(
                        "weak=0x{ptr:x} live=false expr={:?} parent={} created_seq={} context_seq={} dropped_seq={} last_seq={} last_op={}",
                        record.expr,
                        record
                            .parent
                            .map(|p| format!("0x{p:x}"))
                            .unwrap_or_else(|| "none".to_string()),
                        record.created_seq,
                        record.context_seq,
                        record.dropped_seq,
                        record.last_seq,
                        record.last_op,
                    ),
                    None => format!("weak=0x{ptr:x} live=false registry=missing"),
                }
            }
        }
    }

    pub(crate) fn dump(reason: &str) {
        if !enabled() {
            return;
        }
        let state = state();
        if state.dumped.swap(true, Ordering::Relaxed) {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(
                stderr,
                "\n========== ZENOH RESOURCE TRACE DUMP: {} skipped; already dumped ==========" ,
                reason
            );
            return;
        }
        let events: Vec<String> = lock(&state.events).iter().cloned().collect();
        let mut records: Vec<(usize, ResourceRecord)> = lock(&state.registry)
            .iter()
            .map(|(ptr, record)| (*ptr, record.clone()))
            .collect();
        records.sort_by(|a, b| b.1.last_seq.cmp(&a.1.last_seq));

        let mut stderr = io::stderr().lock();
        let _ = writeln!(
            stderr,
            "\n========== ZENOH RESOURCE TRACE DUMP: {} ==========",
            reason
        );
        let _ = writeln!(stderr, "----- recent events ({} kept) -----", events.len());
        for event in events {
            let _ = writeln!(stderr, "{}", event);
        }
        let _ = writeln!(stderr, "----- resource registry ({} records, newest 200) -----", records.len());
        for (ptr, record) in records.into_iter().take(200) {
            let _ = writeln!(
                stderr,
                "ptr=0x{ptr:x} expr={:?} parent={} created_seq={} context_seq={} dropped_seq={} last_seq={} last_op={}",
                record.expr,
                record
                    .parent
                    .map(|p| format!("0x{p:x}"))
                    .unwrap_or_else(|| "none".to_string()),
                record.created_seq,
                record.context_seq,
                record.dropped_seq,
                record.last_seq,
                record.last_op,
            );
        }
        let _ = writeln!(stderr, "========== END ZENOH RESOURCE TRACE DUMP ==========");
    }

    pub(crate) fn dump_dead_weak(reason: &str, owner: Option<&Arc<Resource>>, weak: &Weak<Resource>) {
        if !enabled() {
            return;
        }
        event(format_args!(
            "DEAD_WEAK reason={} owner={} {}",
            reason,
            owner
                .map(arc_summary)
                .unwrap_or_else(|| "none".to_string()),
            weak_summary(weak),
        ));
        dump(reason);
    }
}

use zenoh_collections::{IntHashMap, IntHashSet, SingleOrBoxHashSet};
use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::{key_expr::keyexpr, ExprId, WireExpr},
    network::{
        declare::{ext, queryable::ext::QueryableInfoType, Declare, DeclareBody, DeclareKeyExpr},
        interest::InterestId,
        Mapping, RequestId,
    },
};
use zenoh_sync::{get_mut_unchecked, Cache, CacheValueType};

use super::{
    face::FaceState,
    pubsub::SubscriberInfo,
    tables::{Tables, TablesLock},
};
use crate::net::routing::{
    dispatcher::{
        face::{Face, FaceId},
        tables::RoutingExpr,
    },
    hat::HatTrait,
    interceptor::{InterceptorTrait, InterceptorsChain},
    router::{disable_matches_data_routes, disable_matches_query_routes},
    RoutingContext,
};

pub(crate) type NodeId = u16;

pub(crate) type Direction = (Arc<FaceState>, WireExpr<'static>, NodeId);
pub(crate) type Route = Vec<Direction>;

pub(crate) struct QueryTargetQabl {
    pub(crate) direction: Direction,
    pub(crate) info: Option<QueryableInfoType>,
}

impl QueryTargetQabl {
    pub(crate) fn new(
        (&fid, ctx): (&FaceId, &Arc<SessionContext>),
        expr: &RoutingExpr,
        complete: bool,
    ) -> Option<Self> {
        let qabl = ctx.qabl?;
        let wire_expr = expr.get_best_key(fid);
        Some(Self {
            direction: (ctx.face.clone(), wire_expr.to_owned(), NodeId::default()),
            info: Some(QueryableInfoType {
                complete: complete && qabl.complete,
                // NOTE: local client faces are nearer than remote client faces
                distance: if ctx.face.is_local { 0 } else { 1 },
            }),
        })
    }
}

pub(crate) type QueryTargetQablSet = Vec<QueryTargetQabl>;

/// Helper struct to build route, handling face deduplication.
pub(crate) struct RouteBuilder<T = Direction> {
    /// The route built.
    route: Vec<T>,
    /// The faces' id already inserted.
    faces: IntHashSet<usize>,
}

impl<T> RouteBuilder<T> {
    /// Creates a new empty builder.
    pub(crate) fn new() -> Self {
        Self {
            route: Vec::new(),
            faces: IntHashSet::new(),
        }
    }

    /// Inserts a new direction if it has not been registered for the given face.
    pub(crate) fn insert(&mut self, face_id: usize, direction: impl FnOnce() -> T) {
        if self.faces.insert(face_id) {
            self.route.push(direction());
        }
    }

    pub(crate) fn try_insert(&mut self, face_id: usize, direction: impl FnOnce() -> Option<T>) {
        if !self.faces.contains(&face_id) {
            if let Some(direction) = direction() {
                self.faces.insert(face_id);
                self.route.push(direction);
            }
        }
    }

    /// Build the route, consuming the builder.
    pub(crate) fn build(self) -> Vec<T> {
        self.route
    }
}
pub(crate) type QueryRouteBuilder = RouteBuilder<(Direction, RequestId)>;

pub(crate) struct InterceptorCache(Cache<Option<Box<dyn Any + Send + Sync>>>);
pub(crate) type InterceptorCacheValueType = CacheValueType<Option<Box<dyn Any + Send + Sync>>>;

impl InterceptorCache {
    pub(crate) fn new(value: Option<Box<dyn Any + Send + Sync>>, version: usize) -> Self {
        Self(Cache::<Option<Box<dyn Any + Send + Sync>>>::new(
            value, version,
        ))
    }

    pub(crate) fn empty() -> Self {
        InterceptorCache::new(None, 0)
    }

    #[inline]
    fn value(
        &self,
        interceptor: &InterceptorsChain,
        resource: &Resource,
    ) -> Option<InterceptorCacheValueType> {
        self.0
            .value(interceptor.version, || {
                interceptor.compute_keyexpr_cache(resource.keyexpr()?)
            })
            .ok()
    }
}

pub(crate) struct SessionContext {
    pub(crate) face: Arc<FaceState>,
    pub(crate) local_expr_id: Option<ExprId>,
    pub(crate) remote_expr_id: Option<ExprId>,
    pub(crate) subs: Option<SubscriberInfo>,
    pub(crate) qabl: Option<QueryableInfoType>,
    pub(crate) token: bool,
    pub(crate) subscriber_interest_finalized: bool,
    pub(crate) queryable_interest_finalized: bool,
    pub(crate) in_interceptor_cache: InterceptorCache,
    pub(crate) e_interceptor_cache: InterceptorCache,
}

impl SessionContext {
    pub(crate) fn new(face: Arc<FaceState>) -> Self {
        Self {
            face,
            local_expr_id: None,
            remote_expr_id: None,
            subs: None,
            qabl: None,
            token: false,
            subscriber_interest_finalized: false,
            queryable_interest_finalized: false,
            in_interceptor_cache: InterceptorCache::empty(),
            e_interceptor_cache: InterceptorCache::empty(),
        }
    }
}

/// Global version number for route computation.
/// Use 64bit to not care about rollover.
pub type RoutesVersion = u64;

pub(crate) struct Routes<T> {
    routers: Vec<Option<T>>,
    peers: Vec<Option<T>>,
    clients: Vec<Option<T>>,
    version: u64,
}

impl<T> Default for Routes<T> {
    fn default() -> Self {
        Self {
            routers: Vec::new(),
            peers: Vec::new(),
            clients: Vec::new(),
            version: 0,
        }
    }
}

impl<T> Routes<T> {
    pub(crate) fn clear(&mut self) {
        self.routers.clear();
        self.peers.clear();
        self.clients.clear();
    }

    #[inline]
    pub(crate) fn get_route(
        &self,
        version: RoutesVersion,
        whatami: WhatAmI,
        context: NodeId,
    ) -> Option<&T> {
        if version != self.version {
            return None;
        }
        let routes = match whatami {
            WhatAmI::Router => &self.routers,
            WhatAmI::Peer => &self.peers,
            WhatAmI::Client => &self.clients,
        };
        routes.get(context as usize)?.as_ref()
    }

    #[inline]
    pub(crate) fn set_route(
        &mut self,
        version: RoutesVersion,
        whatami: WhatAmI,
        context: NodeId,
        route: T,
    ) {
        if self.version != version {
            self.clear();
            self.version = version;
        }
        let routes = match whatami {
            WhatAmI::Router => &mut self.routers,
            WhatAmI::Peer => &mut self.peers,
            WhatAmI::Client => &mut self.clients,
        };
        routes.resize_with(context as usize + 1, || None);
        routes[context as usize] = Some(route);
    }
}

pub(crate) fn get_or_set_route<T: Clone>(
    routes: &RwLock<Routes<T>>,
    version: RoutesVersion,
    whatami: WhatAmI,
    context: NodeId,
    compute_route: impl FnOnce() -> T,
) -> T {
    if let Some(route) = routes.read().unwrap().get_route(version, whatami, context) {
        return route.clone();
    }
    let mut routes = routes.write().unwrap();
    if let Some(route) = routes.get_route(version, whatami, context) {
        return route.clone();
    }
    let route = compute_route();
    routes.set_route(version, whatami, context, route.clone());
    route
}

pub(crate) type DataRoutes = Routes<Arc<Route>>;
pub(crate) type QueryRoutes = Routes<Arc<QueryTargetQablSet>>;

pub(crate) struct ResourceContext {
    pub(crate) matches: Vec<Weak<Resource>>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) data_routes: RwLock<DataRoutes>,
    pub(crate) query_routes: RwLock<QueryRoutes>,
    #[cfg(feature = "stats")]
    pub(crate) stats_keys: zenoh_stats::StatsKeyCache,
}

impl ResourceContext {
    fn new(hat: Box<dyn Any + Send + Sync>) -> ResourceContext {
        ResourceContext {
            matches: Vec::new(),
            hat,
            data_routes: Default::default(),
            query_routes: Default::default(),
            #[cfg(feature = "stats")]
            stats_keys: Default::default(),
        }
    }

    pub(crate) fn disable_data_routes(&mut self) {
        self.data_routes.get_mut().unwrap().clear();
    }

    pub(crate) fn disable_query_routes(&mut self) {
        self.query_routes.get_mut().unwrap().clear();
    }
}

pub struct Resource {
    pub(crate) parent: Option<Arc<Resource>>,
    pub(crate) expr: String,
    pub(crate) suffix: usize,
    pub(crate) nonwild_prefix: Option<Arc<Resource>>,
    pub(crate) children: SingleOrBoxHashSet<Child>,
    pub(crate) context: Option<Box<ResourceContext>>,
    pub(crate) session_ctxs: IntHashMap<usize, Arc<SessionContext>>,
}

impl Drop for Resource {
    fn drop(&mut self) {
        resource_trace::mark_drop(self);
    }
}

impl PartialEq for Resource {
    fn eq(&self, other: &Self) -> bool {
        self.expr() == other.expr()
    }
}
impl Eq for Resource {}

// NOTE: The `clippy::mutable_key_type` lint takes issue with the fact that `Resource` contains
// interior mutable data. A configuration option is used to assert that the accessed fields are
// not interior mutable in clippy.toml. Thus care should be taken to ensure soundness of this impl
// as Clippy will not warn about its usage in sets/maps.
impl Hash for Resource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr().hash(state);
    }
}

#[derive(Clone)]
pub(crate) struct Child(Arc<Resource>);

impl Deref for Child {
    type Target = Arc<Resource>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Child {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for Child {
    fn eq(&self, other: &Self) -> bool {
        self.0.suffix() == other.0.suffix()
    }
}

impl Eq for Child {}

impl Hash for Child {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.suffix().hash(state);
    }
}

impl Borrow<str> for Child {
    fn borrow(&self) -> &str {
        self.0.suffix()
    }
}

impl Resource {
    fn new(parent: &Arc<Resource>, suffix: &str, context: Option<ResourceContext>) -> Resource {
        let nonwild_prefix = match &parent.nonwild_prefix {
            None => {
                if suffix.contains('*') {
                    Some(parent.clone())
                } else {
                    None
                }
            }
            Some(prefix) => Some(prefix.clone()),
        };

        Resource {
            parent: Some(parent.clone()),
            expr: parent.expr.clone() + suffix,
            suffix: parent.expr.len(),
            nonwild_prefix,
            children: SingleOrBoxHashSet::new(),
            context: context.map(Box::new),
            session_ctxs: IntHashMap::new(),
        }
    }

    pub fn expr(&self) -> &str {
        &self.expr
    }

    pub fn keyexpr(&self) -> Option<&keyexpr> {
        if self.parent.is_none() {
            None
        } else {
            // SAFETY: non-root resources are valid keyexprs
            unsafe { Some(keyexpr::from_str_unchecked(&self.expr)) }
        }
    }

    pub fn suffix(&self) -> &str {
        &self.expr[self.suffix..]
    }

    #[inline(always)]
    pub(crate) fn context(&self) -> &ResourceContext {
        self.context.as_ref().unwrap()
    }

    #[inline(always)]
    pub(crate) fn context_mut(&mut self) -> &mut ResourceContext {
        self.context.as_mut().unwrap()
    }

    #[inline(always)]
    pub(crate) fn matches(&self, other: &Arc<Resource>) -> bool {
        self.context
            .as_ref()
            .unwrap()
            .matches
            .iter()
            .any(|m| m.upgrade().is_some_and(|m| &m == other))
    }

    pub fn nonwild_prefix(res: &Arc<Resource>) -> (Option<Arc<Resource>>, String) {
        match &res.nonwild_prefix {
            None => (Some(res.clone()), "".to_string()),
            Some(nonwild_prefix) => {
                if !nonwild_prefix.expr().is_empty() {
                    (
                        Some(nonwild_prefix.clone()),
                        res.expr[nonwild_prefix.expr.len()..].to_string(),
                    )
                } else {
                    (None, res.expr().to_string())
                }
            }
        }
    }

    pub fn root() -> Arc<Resource> {
        let res = Arc::new(Resource {
            parent: None,
            expr: String::from(""),
            suffix: 0,
            nonwild_prefix: None,
            children: SingleOrBoxHashSet::new(),
            context: None,
            session_ctxs: IntHashMap::new(),
        });
        resource_trace::mark_created(&res, "root");
        res
    }

    #[track_caller]
    pub fn clean(res: &mut Arc<Resource>) {
        let caller = std::panic::Location::caller();
        if resource_trace::enabled() {
            resource_trace::event(format_args!(
                "CLEAN_ENTER caller={}:{} before_clone strong={} weak={} {}",
                caller.file(),
                caller.line(),
                Arc::strong_count(res),
                Arc::weak_count(res),
                resource_trace::arc_summary(res),
            ));
        }
        let mut resclone = res.clone();
        if resource_trace::enabled() {
            resource_trace::event(format_args!(
                "CLEAN_AFTER_CLONE caller={}:{} strong={} weak={} {}",
                caller.file(),
                caller.line(),
                Arc::strong_count(res),
                Arc::weak_count(res),
                resource_trace::arc_summary(res),
            ));
        }
        let mutres = get_mut_unchecked(&mut resclone);
        if let Some(ref mut parent) = mutres.parent {
            let removable = Arc::strong_count(res) <= 3 && res.children.is_empty();
            if resource_trace::enabled() {
                resource_trace::event(format_args!(
                    "CLEAN_DECISION caller={}:{} removable={} strong={} weak={} childless={} {}",
                    caller.file(),
                    caller.line(),
                    removable,
                    Arc::strong_count(res),
                    Arc::weak_count(res),
                    res.children.is_empty(),
                    resource_trace::arc_summary(res),
                ));
            }
            if removable {
                // consider only childless resource held by only one external object (+ 1 strong count for resclone, + 1 strong count for res.parent to a total of 3 )
                tracing::debug!("Unregister resource {}", res.expr());
                resource_trace::mark_resource_event("CLEAN_REMOVING", res);
                if let Some(context) = mutres.context.as_mut() {
                    for weak in &mut context.matches {
                        if resource_trace::enabled() {
                            resource_trace::event(format_args!(
                                "CLEAN_MATCH_ITER owner={} {}",
                                resource_trace::arc_summary(res),
                                resource_trace::weak_summary(weak),
                            ));
                        }
                        let mut match_ = match weak.upgrade() {
                            Some(match_) => match_,
                            None => {
                                resource_trace::dump_dead_weak(
                                    "dead weak in Resource::clean owner context.matches",
                                    Some(res),
                                    weak,
                                );
                                panic!(
                                    "dead Weak<Resource> in Resource::clean owner context.matches weak=0x{:x} owner={}",
                                    resource_trace::weak_ptr(weak),
                                    res.expr()
                                );
                            }
                        };
                        if !Arc::ptr_eq(&match_, res) {
                            let match_summary = resource_trace::arc_summary(&match_);
                            let match_expr = match_.expr().to_string();
                            let mutmatch = get_mut_unchecked(&mut match_);
                            if let Some(ctx) = mutmatch.context.as_mut() {
                                let before = ctx.matches.len();
                                ctx.matches.retain(|x| match x.upgrade() {
                                    Some(upgraded) => !Arc::ptr_eq(&upgraded, res),
                                    None => {
                                        resource_trace::dump_dead_weak(
                                            "dead weak in Resource::clean reciprocal retain",
                                            None,
                                            x,
                                        );
                                        panic!(
                                            "dead Weak<Resource> in Resource::clean reciprocal retain weak=0x{:x} owner={}",
                                            resource_trace::weak_ptr(x),
                                            match_expr
                                        );
                                    }
                                });
                                if resource_trace::enabled() {
                                    resource_trace::event(format_args!(
                                        "CLEAN_RECIPROCAL_RETAIN owner={} removing={} before={} after={}",
                                        match_summary,
                                        resource_trace::arc_summary(res),
                                        before,
                                        ctx.matches.len(),
                                    ));
                                }
                            }
                        }
                    }
                }
                mutres.nonwild_prefix.take();
                {
                    let parent_mut = get_mut_unchecked(parent);
                    let before = parent_mut.children.iter().count();
                    parent_mut.children.remove(res.suffix());
                    let after = parent_mut.children.iter().count();
                    if resource_trace::enabled() {
                        resource_trace::event(format_args!(
                            "CLEAN_PARENT_CHILD_REMOVE child={} parent={} before={} after={}",
                            resource_trace::arc_summary(res),
                            resource_trace::arc_summary(parent),
                            before,
                            after,
                        ));
                    }
                }
                resource_trace::event(format_args!(
                    "CLEAN_RECURSE_PARENT child={} parent={}",
                    resource_trace::arc_summary(res),
                    resource_trace::arc_summary(parent),
                ));
                Resource::clean(parent);
            }
        }
    }

    pub fn close(self: &mut Arc<Resource>) {
        let r = get_mut_unchecked(self);
        for mut c in r.children.drain() {
            Self::close(&mut c);
        }
        r.parent.take();
        r.nonwild_prefix.take();
        r.context.take();
        r.session_ctxs.clear();
    }

    #[cfg(test)]
    pub fn print_tree(from: &Arc<Resource>) -> String {
        let mut result = from.expr().to_string();
        result.push('\n');
        for child in from.children.iter() {
            result.push_str(&Resource::print_tree(child));
        }
        result
    }

    pub fn make_resource(
        hat_code: &(dyn HatTrait + Send + Sync),
        _tables: &mut Tables,
        from: &mut Arc<Resource>,
        mut suffix: &str,
    ) -> Arc<Resource> {
        if !suffix.is_empty() && !suffix.starts_with('/') {
            if let Some(parent) = &mut from.parent.clone() {
                return Resource::make_resource(
                    hat_code,
                    _tables,
                    parent,
                    &[from.suffix(), suffix].concat(),
                );
            }
        }
        let mut from = from.clone();
        // do not use recursion as the tree may have arbitrary depth
        while let Some((chunk, rest)) = Self::split_first_chunk(suffix) {
            let existing_child = get_mut_unchecked(&mut from)
                .children
                .get(chunk)
                .map(|child| child.0.clone());
            if let Some(child) = existing_child {
                if resource_trace::enabled() {
                    resource_trace::event(format_args!(
                        "MAKE_RESOURCE_EXISTING chunk={:?} from={} child={}",
                        chunk,
                        resource_trace::arc_summary(&from),
                        resource_trace::arc_summary(&child),
                    ));
                }
                from = child;
            } else {
                let new = Arc::new(Resource::new(&from, chunk, None));
                resource_trace::mark_created(&new, "make_resource_child");
                if rest.is_empty() {
                    tracing::debug!("Register resource {}", new.expr());
                }
                if resource_trace::enabled() {
                    resource_trace::event(format_args!(
                        "MAKE_RESOURCE_CHILD_INSERT chunk={:?} parent={} child={}",
                        chunk,
                        resource_trace::arc_summary(&from),
                        resource_trace::arc_summary(&new),
                    ));
                }
                get_mut_unchecked(&mut from)
                    .children
                    .insert(Child(new.clone()));
                from = new;
            };
            suffix = rest;
        }
        Resource::upgrade_resource(&mut from, hat_code.new_resource());
        from
    }

    #[inline]
    pub fn get_resource_ref<'a>(
        mut from: &'a Arc<Resource>,
        mut suffix: &str,
    ) -> Option<&'a Arc<Resource>> {
        if !suffix.is_empty() && !suffix.starts_with('/') {
            if let Some(parent) = &from.parent {
                return Resource::get_resource_ref(parent, &[from.suffix(), suffix].concat());
            }
        }
        // do not use recursion as the tree may have arbitrary depth
        while let Some((chunk, rest)) = Self::split_first_chunk(suffix) {
            (from, suffix) = (from.children.get(chunk)?, rest);
        }
        Some(from)
    }

    #[inline]
    pub fn get_resource(from: &Arc<Resource>, suffix: &str) -> Option<Arc<Resource>> {
        Self::get_resource_ref(from, suffix).cloned()
    }

    /// Split the suffix at the next '/' (after leading one), returning None if the suffix is empty.
    ///
    /// Suffix usually starts with '/', so this first slash is kept as part of the split chunk.
    /// The rest will contain the slash of the split.
    /// For example `split_first_chunk("/a/b") == Some(("/a", "/b"))`.
    fn split_first_chunk(suffix: &str) -> Option<(&str, &str)> {
        if suffix.is_empty() {
            return None;
        }
        // don't count the first char which may be a leading slash to find the next one
        Some(match suffix[1..].find('/') {
            // don't forget to add 1 to the index because of `[1..]` slice above
            Some(idx) => suffix.split_at(idx + 1),
            None => (suffix, ""),
        })
    }

    #[inline]
    pub fn decl_key(
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        push: bool,
    ) -> WireExpr<'static> {
        if face.is_local {
            return res.expr().to_string().into();
        }

        let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(res);
        match nonwild_prefix {
            Some(mut nonwild_prefix) => {
                if let Some(ctx) = get_mut_unchecked(&mut nonwild_prefix)
                    .session_ctxs
                    .get(&face.id)
                {
                    if let Some(expr_id) = ctx.remote_expr_id {
                        return WireExpr {
                            scope: expr_id,
                            suffix: wildsuffix.into(),
                            mapping: Mapping::Receiver,
                        };
                    }
                    if let Some(expr_id) = ctx.local_expr_id {
                        return WireExpr {
                            scope: expr_id,
                            suffix: wildsuffix.into(),
                            mapping: Mapping::Sender,
                        };
                    }
                }
                if push
                    || face.remote_key_interests.values().any(|res| {
                        res.as_ref()
                            .map(|res| res.matches(&nonwild_prefix))
                            .unwrap_or(true)
                    })
                {
                    let ctx = get_mut_unchecked(&mut nonwild_prefix)
                        .session_ctxs
                        .entry(face.id)
                        .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                    let expr_id = face.get_next_local_id();
                    get_mut_unchecked(ctx).local_expr_id = Some(expr_id);
                    get_mut_unchecked(face)
                        .local_mappings
                        .insert(expr_id, nonwild_prefix.clone());
                    face.primitives.send_declare(RoutingContext::with_expr(
                        &mut Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                                id: expr_id,
                                wire_expr: nonwild_prefix.expr().to_string().into(),
                            }),
                        },
                        nonwild_prefix.expr().to_string(),
                    ));
                    face.update_interceptors_caches(&mut nonwild_prefix);
                    WireExpr {
                        scope: expr_id,
                        suffix: wildsuffix.into(),
                        mapping: Mapping::Sender,
                    }
                } else {
                    res.expr().to_string().into()
                }
            }
            None => wildsuffix.into(),
        }
    }

    /// Return the best locally/remotely declared keyexpr, i.e. with the smallest suffix, matching
    /// the given suffix and session id.
    ///
    /// The goal is to save bandwidth by using the shortest keyexpr on the wire. It works by
    /// recursively walk through the children tree, looking for an already declared keyexpr for the
    /// session.
    /// If none is found, and if the tested resource itself doesn't have a declared keyexpr,
    /// then the parent tree is walked through. If there is still no declared keyexpr, the whole
    /// prefix+suffix string is used.
    pub fn get_best_key<'a>(&self, suffix: &'a str, sid: usize) -> WireExpr<'a> {
        /// Retrieve a declared keyexpr, either local or remote.
        fn get_wire_expr<'a>(
            prefix: &Resource,
            suffix: impl FnOnce() -> Cow<'a, str>,
            sid: usize,
        ) -> Option<WireExpr<'a>> {
            let ctx = prefix.session_ctxs.get(&sid)?;
            let (scope, mapping) = match (ctx.remote_expr_id, ctx.local_expr_id) {
                (Some(expr_id), _) => (expr_id, Mapping::Receiver),
                (_, Some(expr_id)) => (expr_id, Mapping::Sender),
                _ => return None,
            };
            Some(WireExpr {
                scope,
                suffix: suffix(),
                mapping,
            })
        }
        /// Walk through the children tree, looking for a declared keyexpr.
        fn get_best_child_key<'a>(
            mut prefix: &Resource,
            suffix: &'a str,
            sid: usize,
        ) -> Option<WireExpr<'a>> {
            let mut suffix_rest = suffix;
            // do not use recursion as the tree may have arbitrary depth
            // first we get the closest matching child
            while let Some((chunk, rest)) = Resource::split_first_chunk(suffix_rest) {
                match prefix.children.get(chunk) {
                    Some(child) => prefix = child,
                    None => break,
                }
                suffix_rest = rest;
            }
            // then we go backward checking the child and its parents
            while suffix_rest != suffix {
                if let Some(wire_expr) = get_wire_expr(prefix, || suffix_rest.into(), sid) {
                    return Some(wire_expr);
                }
                suffix_rest = &suffix[suffix.len() - suffix_rest.len() - prefix.suffix().len()..];
                prefix = prefix.parent.as_ref().unwrap();
            }
            None
        }
        /// Walk through the parent tree, looking for a declared keyexpr.
        fn get_best_parent_key<'a>(
            prefix: &Resource,
            suffix: &'a str,
            sid: usize,
            mut parent: &Resource,
        ) -> Option<WireExpr<'a>> {
            // do not use recursion as the tree may have arbitrary depth
            loop {
                let parent_suffix = || [&prefix.expr[parent.expr.len()..], suffix].concat().into();
                if let Some(wire_expr) = get_wire_expr(parent, parent_suffix, sid) {
                    return Some(wire_expr);
                }
                match parent.parent.as_ref() {
                    Some(p) => parent = p,
                    None => return None,
                }
            }
        }
        get_best_child_key(self, suffix, sid)
            .or_else(|| get_wire_expr(self, || suffix.into(), sid))
            .or_else(|| get_best_parent_key(self, suffix, sid, self.parent.as_ref()?))
            .unwrap_or_else(|| [&self.expr, suffix].concat().into())
    }

    pub fn get_matches(tables: &Tables, key_expr: &keyexpr) -> Vec<Weak<Resource>> {
        pub fn visit_nodes<T>(node: T, mut visit: impl FnMut(T, &mut VecDeque<T>)) {
            let mut nodes = VecDeque::from([node]);
            while let Some(node) = nodes.pop_front() {
                visit(node, &mut nodes);
            }
        }
        fn get_matches_from(
            key_expr: &keyexpr,
            from: &Arc<Resource>,
            matches: &mut Vec<Weak<Resource>>,
        ) {
            visit_nodes((key_expr, from), |(key_expr, from), nodes| {
                if from.parent.is_none() || from.suffix() == "/" {
                    for child in from.children.iter() {
                        nodes.push_back((key_expr, child));
                    }
                    return;
                }
                let suffix: &keyexpr = from
                    .suffix()
                    .strip_prefix('/')
                    .unwrap_or(from.suffix())
                    .try_into()
                    .unwrap();
                let (ke_chunk, ke_rest) = match key_expr.split_once('/') {
                    // SAFETY: chunks of keyexpr are valid keyexprs
                    Some((chunk, rest)) => unsafe {
                        (
                            keyexpr::from_str_unchecked(chunk),
                            Some(keyexpr::from_str_unchecked(rest)),
                        )
                    },
                    None => (key_expr, None),
                };
                let ke_chunk_intersects_suffix = ke_chunk.intersects(suffix);
                let ke_chunk_is_wild = ke_chunk.as_bytes() == b"**";
                let suffix_is_wild = suffix.as_bytes() == b"**";
                match ke_rest {
                    None => {
                        if ke_chunk_intersects_suffix {
                            if from.context.is_some() {
                                matches.push(Arc::downgrade(from));
                            }
                            if let Some(child) =
                                from.children.get("/**").or_else(|| from.children.get("**"))
                            {
                                if child.context.is_some() {
                                    matches.push(Arc::downgrade(child))
                                }
                            }
                        }
                        if (ke_chunk_is_wild && ke_chunk_intersects_suffix) || suffix_is_wild {
                            for child in from.children.iter() {
                                nodes.push_back((key_expr, child));
                            }
                        }
                    }
                    Some(rest) => {
                        if ke_chunk_intersects_suffix
                            && rest.as_bytes() == b"**"
                            && from.context.is_some()
                        {
                            matches.push(Arc::downgrade(from));
                        }
                        for child in from.children.iter() {
                            if (ke_chunk_is_wild && ke_chunk_intersects_suffix) || suffix_is_wild {
                                nodes.push_back((key_expr, child));
                            } else if ke_chunk_intersects_suffix {
                                nodes.push_back((rest, child));
                            }
                        }
                        if (suffix_is_wild && ke_chunk_intersects_suffix) || ke_chunk_is_wild {
                            nodes.push_back((rest, from));
                        }
                    }
                };
            })
        }
        let mut matches = Vec::new();
        get_matches_from(key_expr, &tables.root_res, &mut matches);
        matches.sort_unstable_by_key(Weak::as_ptr);
        matches.dedup_by_key(|res| Weak::as_ptr(res));
        if resource_trace::enabled() {
            let summaries = matches
                .iter()
                .map(resource_trace::weak_summary)
                .collect::<Vec<_>>()
                .join(", ");
            resource_trace::event(format_args!(
                "GET_MATCHES key_expr={} count={} [{}]",
                key_expr.as_str(),
                matches.len(),
                summaries,
            ));
        }
        matches
    }

    #[track_caller]
    pub fn match_resource(_tables: &Tables, res: &mut Arc<Resource>, matches: Vec<Weak<Resource>>) {
        let caller = std::panic::Location::caller();
        if res.context.is_some() {
            if resource_trace::enabled() {
                let summaries = matches
                    .iter()
                    .map(resource_trace::weak_summary)
                    .collect::<Vec<_>>()
                    .join(", ");
                resource_trace::event(format_args!(
                    "MATCH_RESOURCE_BEGIN caller={}:{} target={} count={} [{}]",
                    caller.file(),
                    caller.line(),
                    resource_trace::arc_summary(res),
                    matches.len(),
                    summaries,
                ));
            }
            for weak in &matches {
                let mut match_ = match weak.upgrade() {
                    Some(match_) => match_,
                    None => {
                        resource_trace::dump_dead_weak(
                            "dead weak in Resource::match_resource input matches",
                            Some(res),
                            weak,
                        );
                        panic!(
                            "dead Weak<Resource> in Resource::match_resource weak=0x{:x} target={}",
                            resource_trace::weak_ptr(weak),
                            res.expr()
                        );
                    }
                };
                if resource_trace::enabled() {
                    resource_trace::event(format_args!(
                        "MATCH_RESOURCE_EDGE_PUSH matched={} target={}",
                        resource_trace::arc_summary(&match_),
                        resource_trace::arc_summary(res),
                    ));
                }
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .matches
                    .push(Arc::downgrade(res));
            }
            get_mut_unchecked(res).context_mut().matches = matches;
            resource_trace::mark_resource_event("MATCH_RESOURCE_ASSIGN_TARGET_MATCHES", res);
        } else {
            tracing::error!("Call match_resource() on context less res {}", res.expr());
            resource_trace::event(format_args!(
                "MATCH_RESOURCE_CONTEXTLESS caller={}:{} target={}",
                caller.file(),
                caller.line(),
                resource_trace::arc_summary(res),
            ));
        }
    }

    pub fn upgrade_resource(res: &mut Arc<Resource>, hat: Box<dyn Any + Send + Sync>) {
        if res.context.is_none() {
            resource_trace::mark_resource_event("RESOURCE_CONTEXT_ATTACH_BEGIN", res);
            get_mut_unchecked(res).context = Some(Box::new(ResourceContext::new(hat)));
            resource_trace::mark_context_attached(res, "upgrade_resource");
        } else {
            resource_trace::mark_resource_event("RESOURCE_CONTEXT_ATTACH_SKIPPED_ALREADY_PRESENT", res);
        }
    }

    pub(crate) fn get_ingress_cache(
        &self,
        face: &Face,
        interceptor: &InterceptorsChain,
    ) -> Option<InterceptorCacheValueType> {
        self.session_ctxs
            .get(&face.state.id)
            .and_then(|ctx| ctx.in_interceptor_cache.value(interceptor, self))
    }

    pub(crate) fn get_egress_cache(
        &self,
        face: &Face,
        interceptor: &InterceptorsChain,
    ) -> Option<InterceptorCacheValueType> {
        self.session_ctxs
            .get(&face.state.id)
            .and_then(|ctx| ctx.e_interceptor_cache.value(interceptor, self))
    }
}

pub(crate) fn register_expr(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr_id: ExprId,
    expr: &WireExpr,
) {
    resource_trace::event(format_args!(
        "REGISTER_EXPR_ENTER face_id={} expr_id={} scope={} suffix={:?}",
        face.id,
        expr_id,
        expr.scope,
        expr.suffix.as_ref(),
    ));
    let rtables = zread!(tables.tables);
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => match face.remote_mappings.get(&expr_id) {
            Some(res) => {
                let mut fullexpr = prefix.expr().to_string();
                fullexpr.push_str(expr.suffix.as_ref());
                if res.expr() != fullexpr {
                    tracing::error!(
                        "{} Resource {} remapped. Remapping unsupported!",
                        face,
                        expr_id
                    );
                }
            }
            None => {
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) =
                    if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                        drop(rtables);
                        let wtables = zwrite!(tables.tables);
                        (res.unwrap(), wtables)
                    } else {
                        let mut fullexpr = prefix.expr().to_string();
                        fullexpr.push_str(expr.suffix.as_ref());
                        let mut matches = keyexpr::new(fullexpr.as_str())
                            .map(|ke| Resource::get_matches(&rtables, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut wtables = zwrite!(tables.tables);
                        let mut res = Resource::make_resource(
                            tables.hat_code.as_ref(),
                            &mut wtables,
                            &mut prefix,
                            expr.suffix.as_ref(),
                        );
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&wtables, &mut res, matches);
                        (res, wtables)
                    };
                let ctx = get_mut_unchecked(&mut res)
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));

                get_mut_unchecked(ctx).remote_expr_id = Some(expr_id);

                get_mut_unchecked(face)
                    .remote_mappings
                    .insert(expr_id, res.clone());
                disable_matches_data_routes(&mut wtables, &mut res);
                disable_matches_query_routes(&mut wtables, &mut res);
                face.update_interceptors_caches(&mut res);
                drop(wtables);
            }
        },
        None => tracing::error!(
            "{} Declare resource with unknown scope {}!",
            face,
            expr.scope
        ),
    }
}

pub(crate) fn unregister_expr(tables: &TablesLock, face: &mut Arc<FaceState>, expr_id: ExprId) {
    resource_trace::event(format_args!(
        "UNREGISTER_EXPR_ENTER face_id={} expr_id={}",
        face.id,
        expr_id,
    ));
    let wtables = zwrite!(tables.tables);
    match get_mut_unchecked(face).remote_mappings.remove(&expr_id) {
        Some(mut res) => {
            resource_trace::event(format_args!(
                "UNREGISTER_EXPR_REMOVE_MAPPING face_id={} expr_id={} res={}",
                face.id,
                expr_id,
                resource_trace::arc_summary(&res),
            ));
            Resource::clean(&mut res)
        }
        None => tracing::error!("{} Undeclare unknown resource!", face),
    }
    drop(wtables);
}

pub(crate) fn register_expr_interest(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
    expr: Option<&WireExpr>,
) {
    resource_trace::event(format_args!(
        "REGISTER_EXPR_INTEREST_ENTER face_id={} interest_id={} expr={}",
        face.id,
        id,
        expr.map(|expr| format!("scope={} suffix={:?}", expr.scope, expr.suffix.as_ref()))
            .unwrap_or_else(|| "none".to_string()),
    ));
    if let Some(expr) = expr {
        let rtables = zread!(tables.tables);
        match rtables
            .get_mapping(face, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (res, wtables) = if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr().to_string();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res = Resource::make_resource(
                        tables.hat_code.as_ref(),
                        &mut wtables,
                        &mut prefix,
                        expr.suffix.as_ref(),
                    );
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };
                get_mut_unchecked(face)
                    .remote_key_interests
                    .insert(id, Some(res));
                drop(wtables);
            }
            None => tracing::error!(
                "{} Declare keyexpr interest with unknown scope {}!",
                face,
                expr.scope,
            ),
        }
    } else {
        let wtables = zwrite!(tables.tables);
        get_mut_unchecked(face)
            .remote_key_interests
            .insert(id, None);
        drop(wtables);
    }
}

pub(crate) fn unregister_expr_interest(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
) {
    resource_trace::event(format_args!(
        "UNREGISTER_EXPR_INTEREST_ENTER face_id={} interest_id={}",
        face.id,
        id,
    ));
    let wtables = zwrite!(tables.tables);
    get_mut_unchecked(face).remote_key_interests.remove(&id);
    drop(wtables);
}
