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
use super::router::{Tables, TablesLock};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Weak};
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    core::{key_expr::keyexpr, QueryableInfo, SubInfo, WireExpr, ZInt, ZenohId},
    zenoh::{DataInfo, RoutingContext},
};
use zenoh_sync::get_mut_unchecked;

pub(super) type Direction = (Arc<FaceState>, WireExpr<'static>, Option<RoutingContext>);
pub(super) type Route = HashMap<usize, Direction>;
#[cfg(feature = "complete_n")]
pub(super) type QueryRoute = HashMap<usize, (Direction, ZInt, zenoh_protocol::core::QueryTarget)>;
#[cfg(not(feature = "complete_n"))]
pub(super) type QueryRoute = HashMap<usize, (Direction, ZInt)>;
pub(super) struct QueryTargetQabl {
    pub(super) direction: Direction,
    pub(super) complete: ZInt,
    pub(super) distance: f64,
}
pub(super) type QueryTargetQablSet = Vec<QueryTargetQabl>;
pub(super) type PullCaches = Vec<Arc<SessionContext>>;

pub(super) struct SessionContext {
    pub(super) face: Arc<FaceState>,
    pub(super) local_expr_id: Option<ZInt>,
    pub(super) remote_expr_id: Option<ZInt>,
    pub(super) subs: Option<SubInfo>,
    pub(super) qabl: Option<QueryableInfo>,
    pub(super) last_values: HashMap<String, (Option<DataInfo>, ZBuf)>,
}

pub(super) struct DataRoutes {
    pub(super) matching_pulls: Option<Arc<PullCaches>>,
    pub(super) routers_data_routes: Vec<Arc<Route>>,
    pub(super) peers_data_routes: Vec<Arc<Route>>,
    pub(super) peer_data_route: Option<Arc<Route>>,
    pub(super) client_data_route: Option<Arc<Route>>,
}

pub(super) struct QueryRoutes {
    pub(super) routers_query_routes: Vec<Arc<QueryTargetQablSet>>,
    pub(super) peers_query_routes: Vec<Arc<QueryTargetQablSet>>,
    pub(super) peer_query_route: Option<Arc<QueryTargetQablSet>>,
    pub(super) client_query_route: Option<Arc<QueryTargetQablSet>>,
}

pub(super) struct ResourceContext {
    pub(super) router_subs: HashSet<ZenohId>,
    pub(super) peer_subs: HashSet<ZenohId>,
    pub(super) router_qabls: HashMap<ZenohId, QueryableInfo>,
    pub(super) peer_qabls: HashMap<ZenohId, QueryableInfo>,
    pub(super) matches: Vec<Weak<Resource>>,
    pub(super) matching_pulls: Arc<PullCaches>,
    pub(super) valid_data_routes: bool,
    pub(super) routers_data_routes: Vec<Arc<Route>>,
    pub(super) peers_data_routes: Vec<Arc<Route>>,
    pub(super) peer_data_route: Option<Arc<Route>>,
    pub(super) client_data_route: Option<Arc<Route>>,
    pub(super) valid_query_routes: bool,
    pub(super) routers_query_routes: Vec<Arc<QueryTargetQablSet>>,
    pub(super) peers_query_routes: Vec<Arc<QueryTargetQablSet>>,
    pub(super) peer_query_route: Option<Arc<QueryTargetQablSet>>,
    pub(super) client_query_route: Option<Arc<QueryTargetQablSet>>,
}

impl ResourceContext {
    fn new() -> ResourceContext {
        ResourceContext {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            peer_qabls: HashMap::new(),
            matches: Vec::new(),
            matching_pulls: Arc::new(Vec::new()),
            valid_data_routes: false,
            routers_data_routes: Vec::new(),
            peers_data_routes: Vec::new(),
            peer_data_route: None,
            client_data_route: None,
            valid_query_routes: false,
            routers_query_routes: Vec::new(),
            peers_query_routes: Vec::new(),
            peer_query_route: None,
            client_query_route: None,
        }
    }

    pub(super) fn update_data_routes(&mut self, data_routes: DataRoutes) {
        self.valid_data_routes = true;
        if let Some(matching_pulls) = data_routes.matching_pulls {
            self.matching_pulls = matching_pulls;
        }
        self.routers_data_routes = data_routes.routers_data_routes;
        self.peers_data_routes = data_routes.peers_data_routes;
        self.peer_data_route = data_routes.peer_data_route;
        self.client_data_route = data_routes.client_data_route;
    }

    pub(super) fn update_query_routes(&mut self, query_routes: QueryRoutes) {
        self.valid_query_routes = true;
        self.routers_query_routes = query_routes.routers_query_routes;
        self.peers_query_routes = query_routes.peers_query_routes;
        self.peer_query_route = query_routes.peer_query_route;
        self.client_query_route = query_routes.client_query_route;
    }
}

pub struct Resource {
    pub(super) parent: Option<Arc<Resource>>,
    pub(super) suffix: String,
    pub(super) nonwild_prefix: Option<(Arc<Resource>, String)>,
    pub(super) childs: HashMap<String, Arc<Resource>>,
    pub(super) context: Option<ResourceContext>,
    pub(super) session_ctxs: HashMap<usize, Arc<SessionContext>>,
}

impl PartialEq for Resource {
    fn eq(&self, other: &Self) -> bool {
        self.expr() == other.expr()
    }
}
impl Eq for Resource {}

impl Hash for Resource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr().hash(state);
    }
}

impl Resource {
    fn new(parent: &Arc<Resource>, suffix: &str, context: Option<ResourceContext>) -> Resource {
        let nonwild_prefix = match &parent.nonwild_prefix {
            None => {
                if suffix.contains('*') {
                    Some((parent.clone(), String::from(suffix)))
                } else {
                    None
                }
            }
            Some((prefix, wildsuffix)) => Some((prefix.clone(), [wildsuffix, suffix].concat())),
        };

        Resource {
            parent: Some(parent.clone()),
            suffix: String::from(suffix),
            nonwild_prefix,
            childs: HashMap::new(),
            context,
            session_ctxs: HashMap::new(),
        }
    }

    pub fn expr(&self) -> String {
        match &self.parent {
            Some(parent) => parent.expr() + &self.suffix,
            None => String::from(""),
        }
    }

    #[inline(always)]
    pub(super) fn context(&self) -> &ResourceContext {
        self.context.as_ref().unwrap()
    }

    #[inline(always)]
    pub(super) fn context_mut(&mut self) -> &mut ResourceContext {
        self.context.as_mut().unwrap()
    }

    pub fn nonwild_prefix(res: &Arc<Resource>) -> (Option<Arc<Resource>>, String) {
        match &res.nonwild_prefix {
            None => (Some(res.clone()), "".to_string()),
            Some((nonwild_prefix, wildsuffix)) => {
                if !nonwild_prefix.expr().is_empty() {
                    (Some(nonwild_prefix.clone()), wildsuffix.clone())
                } else {
                    (None, res.expr())
                }
            }
        }
    }

    #[inline(always)]
    pub fn routers_data_route(&self, context: usize) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_data_routes {
                    (ctx.routers_data_routes.len() > context)
                        .then(|| ctx.routers_data_routes[context].clone())
                } else {
                    None
                }
            }

            None => None,
        }
    }

    #[inline(always)]
    pub fn peers_data_route(&self, context: usize) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_data_routes {
                    (ctx.peers_data_routes.len() > context)
                        .then(|| ctx.peers_data_routes[context].clone())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub fn peer_data_route(&self) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_data_routes {
                    ctx.peer_data_route.clone()
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub fn client_data_route(&self) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_data_routes {
                    ctx.client_data_route.clone()
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn routers_query_route(&self, context: usize) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_query_routes {
                    (ctx.routers_query_routes.len() > context)
                        .then(|| ctx.routers_query_routes[context].clone())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn peers_query_route(&self, context: usize) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_query_routes {
                    (ctx.peers_query_routes.len() > context)
                        .then(|| ctx.peers_query_routes[context].clone())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn peer_query_route(&self) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_query_routes {
                    ctx.peer_query_route.clone()
                } else {
                    None
                }
            }
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn client_query_route(&self) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_query_routes {
                    ctx.client_query_route.clone()
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn root() -> Arc<Resource> {
        Arc::new(Resource {
            parent: None,
            suffix: String::from(""),
            nonwild_prefix: None,
            childs: HashMap::new(),
            context: None,
            session_ctxs: HashMap::new(),
        })
    }

    pub fn clean(res: &mut Arc<Resource>) {
        let mut resclone = res.clone();
        let mutres = get_mut_unchecked(&mut resclone);
        if let Some(ref mut parent) = mutres.parent {
            if Arc::strong_count(res) <= 3 && res.childs.is_empty() {
                log::debug!("Unregister resource {}", res.expr());
                if let Some(context) = mutres.context.as_mut() {
                    for match_ in &mut context.matches {
                        let mut match_ = match_.upgrade().unwrap();
                        if !Arc::ptr_eq(&match_, res) {
                            let mutmatch = get_mut_unchecked(&mut match_);
                            if let Some(ctx) = mutmatch.context.as_mut() {
                                ctx.matches
                                    .retain(|x| !Arc::ptr_eq(&x.upgrade().unwrap(), res));
                            }
                        }
                    }
                }
                {
                    get_mut_unchecked(parent).childs.remove(&res.suffix);
                }
                Resource::clean(parent);
            }
        }
    }

    pub fn print_tree(from: &Arc<Resource>) -> String {
        let mut result = from.expr();
        result.push('\n');
        for child in from.childs.values() {
            result.push_str(&Resource::print_tree(child));
        }
        result
    }

    pub fn make_resource(
        _tables: &mut Tables,
        from: &mut Arc<Resource>,
        suffix: &str,
    ) -> Arc<Resource> {
        if suffix.is_empty() {
            Resource::upgrade_resource(from);
            from.clone()
        } else if let Some(stripped_suffix) = suffix.strip_prefix('/') {
            let (chunk, rest) = match stripped_suffix.find('/') {
                Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                None => (suffix, ""),
            };

            match get_mut_unchecked(from).childs.get_mut(chunk) {
                Some(res) => Resource::make_resource(_tables, res, rest),
                None => {
                    let mut new = Arc::new(Resource::new(from, chunk, None));
                    if log::log_enabled!(log::Level::Debug) && rest.is_empty() {
                        log::debug!("Register resource {}", new.expr());
                    }
                    let res = Resource::make_resource(_tables, &mut new, rest);
                    get_mut_unchecked(from)
                        .childs
                        .insert(String::from(chunk), new);
                    res
                }
            }
        } else {
            match from.parent.clone() {
                Some(mut parent) => {
                    Resource::make_resource(_tables, &mut parent, &[&from.suffix, suffix].concat())
                }
                None => {
                    let (chunk, rest) = match suffix[1..].find('/') {
                        Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                        None => (suffix, ""),
                    };

                    match get_mut_unchecked(from).childs.get_mut(chunk) {
                        Some(res) => Resource::make_resource(_tables, res, rest),
                        None => {
                            let mut new = Arc::new(Resource::new(from, chunk, None));
                            if log::log_enabled!(log::Level::Debug) && rest.is_empty() {
                                log::debug!("Register resource {}", new.expr());
                            }
                            let res = Resource::make_resource(_tables, &mut new, rest);
                            get_mut_unchecked(from)
                                .childs
                                .insert(String::from(chunk), new);
                            res
                        }
                    }
                }
            }
        }
    }

    #[inline]
    pub fn get_resource(from: &Arc<Resource>, suffix: &str) -> Option<Arc<Resource>> {
        if suffix.is_empty() {
            Some(from.clone())
        } else if let Some(stripped_suffix) = suffix.strip_prefix('/') {
            let (chunk, rest) = match stripped_suffix.find('/') {
                Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                None => (suffix, ""),
            };

            match from.childs.get(chunk) {
                Some(res) => Resource::get_resource(res, rest),
                None => None,
            }
        } else {
            match &from.parent {
                Some(parent) => Resource::get_resource(parent, &[&from.suffix, suffix].concat()),
                None => {
                    let (chunk, rest) = match suffix[1..].find('/') {
                        Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                        None => (suffix, ""),
                    };

                    match from.childs.get(chunk) {
                        Some(res) => Resource::get_resource(res, rest),
                        None => None,
                    }
                }
            }
        }
    }

    fn fst_chunk(key_expr: &keyexpr) -> (&keyexpr, Option<&keyexpr>) {
        match key_expr.as_bytes().iter().position(|c| *c == b'/') {
            Some(pos) => {
                let left = &key_expr.as_bytes()[..pos];
                let right = &key_expr.as_bytes()[pos + 1..];
                unsafe {
                    (
                        keyexpr::from_slice_unchecked(left),
                        Some(keyexpr::from_slice_unchecked(right)),
                    )
                }
            }
            None => (key_expr, None),
        }
    }

    #[inline]
    pub fn decl_key(res: &Arc<Resource>, face: &mut Arc<FaceState>) -> WireExpr<'static> {
        let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(res);
        match nonwild_prefix {
            Some(mut nonwild_prefix) => {
                let ctx = get_mut_unchecked(&mut nonwild_prefix)
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| {
                        Arc::new(SessionContext {
                            face: face.clone(),
                            local_expr_id: None,
                            remote_expr_id: None,
                            subs: None,
                            qabl: None,
                            last_values: HashMap::new(),
                        })
                    });

                let expr_id = match ctx.local_expr_id.or(ctx.remote_expr_id) {
                    Some(expr_id) => expr_id,
                    None => {
                        let expr_id = face.get_next_local_id();
                        get_mut_unchecked(ctx).local_expr_id = Some(expr_id);
                        get_mut_unchecked(face)
                            .local_mappings
                            .insert(expr_id, nonwild_prefix.clone());
                        face.primitives
                            .decl_resource(expr_id, &nonwild_prefix.expr().into());
                        expr_id
                    }
                };
                WireExpr {
                    scope: expr_id,
                    suffix: wildsuffix.into(),
                }
            }
            None => wildsuffix.into(),
        }
    }

    #[inline]
    pub fn get_best_key<'a>(prefix: &Arc<Resource>, suffix: &'a str, sid: usize) -> WireExpr<'a> {
        fn get_best_key_<'a>(
            prefix: &Arc<Resource>,
            suffix: &'a str,
            sid: usize,
            checkchilds: bool,
        ) -> WireExpr<'a> {
            if checkchilds && !suffix.is_empty() {
                let (chunk, rest) = suffix.split_at(suffix.find('/').unwrap_or(suffix.len()));
                if let Some(child) = prefix.childs.get(chunk) {
                    return get_best_key_(child, rest, sid, true);
                }
            }
            if let Some(ctx) = prefix.session_ctxs.get(&sid) {
                if let Some(expr_id) = ctx.local_expr_id {
                    return WireExpr {
                        scope: expr_id,
                        suffix: suffix.into(),
                    };
                } else if let Some(expr_id) = ctx.remote_expr_id {
                    return WireExpr {
                        scope: expr_id,
                        suffix: suffix.into(),
                    };
                }
            }
            match &prefix.parent {
                Some(parent) => {
                    get_best_key_(parent, &[&prefix.suffix, suffix].concat(), sid, false).to_owned()
                }
                None => suffix.into(),
            }
        }
        get_best_key_(prefix, suffix, sid, true)
    }

    pub fn get_matches(tables: &Tables, key_expr: &keyexpr) -> Vec<Weak<Resource>> {
        fn recursive_push(from: &Arc<Resource>, matches: &mut Vec<Weak<Resource>>) {
            if from.context.is_some() {
                matches.push(Arc::downgrade(from));
            }
            for child in from.childs.values() {
                recursive_push(child, matches)
            }
        }
        fn get_matches_from(
            key_expr: &keyexpr,
            from: &Arc<Resource>,
            matches: &mut Vec<Weak<Resource>>,
        ) {
            if from.parent.is_none() || from.suffix == "/" {
                for child in from.childs.values() {
                    get_matches_from(key_expr, child, matches);
                }
                return;
            }
            let suffix: &keyexpr = from
                .suffix
                .strip_prefix('/')
                .unwrap_or(&from.suffix)
                .try_into()
                .unwrap();
            let (chunk, rest) = Resource::fst_chunk(key_expr);
            if chunk.intersects(suffix) {
                match rest {
                    None => {
                        if chunk.as_bytes() == b"**" {
                            recursive_push(from, matches)
                        } else {
                            if from.context.is_some() {
                                matches.push(Arc::downgrade(from));
                            }
                            if suffix.as_bytes() == b"**" {
                                for child in from.childs.values() {
                                    get_matches_from(key_expr, child, matches)
                                }
                            }
                            if let Some(child) =
                                from.childs.get("/**").or_else(|| from.childs.get("**"))
                            {
                                if child.context.is_some() {
                                    matches.push(Arc::downgrade(child))
                                }
                            }
                        }
                    }
                    Some(rest) if rest.as_bytes() == b"**" => recursive_push(from, matches),
                    Some(rest) => {
                        let recheck_keyexpr_one_level_lower =
                            chunk.as_bytes() == b"**" || suffix.as_bytes() == b"**";
                        for child in from.childs.values() {
                            get_matches_from(rest, child, matches);
                            if recheck_keyexpr_one_level_lower {
                                get_matches_from(key_expr, child, matches)
                            }
                        }
                        if recheck_keyexpr_one_level_lower {
                            get_matches_from(rest, from, matches)
                        }
                    }
                };
            }
        }
        let mut matches = Vec::new();
        get_matches_from(key_expr, &tables.root_res, &mut matches);
        let mut i = 0;
        while i < matches.len() {
            let current = matches[i].as_ptr();
            let mut j = i + 1;
            while j < matches.len() {
                if std::ptr::eq(current, matches[j].as_ptr()) {
                    matches.swap_remove(j);
                } else {
                    j += 1
                }
            }
            i += 1
        }
        matches
    }

    pub fn match_resource(_tables: &Tables, res: &mut Arc<Resource>, matches: Vec<Weak<Resource>>) {
        if res.context.is_some() {
            for match_ in &matches {
                let mut match_ = match_.upgrade().unwrap();
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .matches
                    .push(Arc::downgrade(res));
            }
            get_mut_unchecked(res).context_mut().matches = matches;
        } else {
            log::error!("Call match_resource() on context less res {}", res.expr());
        }
    }

    pub fn upgrade_resource(res: &mut Arc<Resource>) {
        if res.context.is_none() {
            get_mut_unchecked(res).context = Some(ResourceContext::new());
        }
    }
}

pub fn register_expr(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr_id: ZInt,
    expr: &WireExpr,
) {
    let rtables = zread!(tables.tables);
    match rtables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => match face.remote_mappings.get(&expr_id) {
            Some(res) => {
                let mut fullexpr = prefix.expr();
                fullexpr.push_str(expr.suffix.as_ref());
                if res.expr() != fullexpr {
                    log::error!("Resource {} remapped. Remapping unsupported!", expr_id);
                }
            }
            None => {
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) = if res
                    .as_ref()
                    .map(|r| r.context.is_some())
                    .unwrap_or(false)
                {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };
                let mut ctx = get_mut_unchecked(&mut res)
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| {
                        Arc::new(SessionContext {
                            face: face.clone(),
                            local_expr_id: None,
                            remote_expr_id: Some(expr_id),
                            subs: None,
                            qabl: None,
                            last_values: HashMap::new(),
                        })
                    })
                    .clone();

                if face.local_mappings.get(&expr_id).is_some() && ctx.local_expr_id.is_none() {
                    let local_expr_id = get_mut_unchecked(face).get_next_local_id();
                    get_mut_unchecked(&mut ctx).local_expr_id = Some(local_expr_id);

                    get_mut_unchecked(face)
                        .local_mappings
                        .insert(local_expr_id, res.clone());

                    face.primitives
                        .decl_resource(local_expr_id, &res.expr().into());
                }

                get_mut_unchecked(face)
                    .remote_mappings
                    .insert(expr_id, res.clone());
                wtables.compute_matches_routes(&mut res);
                drop(wtables);
            }
        },
        None => log::error!("Declare resource with unknown scope {}!", expr.scope),
    }
}

pub fn unregister_expr(tables: &TablesLock, face: &mut Arc<FaceState>, expr_id: ZInt) {
    let wtables = zwrite!(tables.tables);
    match get_mut_unchecked(face).remote_mappings.remove(&expr_id) {
        Some(mut res) => Resource::clean(&mut res),
        None => log::error!("Undeclare unknown resource!"),
    }
    drop(wtables);
}
