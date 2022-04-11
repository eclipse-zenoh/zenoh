//
// Copyright (c) 2022 ZettaScale Technology
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
use super::router::Tables;
use async_std::sync::{Arc, Weak};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use zenoh_protocol::io::ZBuf;
use zenoh_protocol::proto::{DataInfo, RoutingContext};
use zenoh_protocol_core::key_expr;
use zenoh_protocol_core::{KeyExpr, PeerId, QueryableInfo, SubInfo, ZInt};
use zenoh_sync::get_mut_unchecked;

pub(super) type Direction = (Arc<FaceState>, KeyExpr<'static>, Option<RoutingContext>);
pub(super) type Route = HashMap<usize, Direction>;
#[cfg(feature = "complete_n")]
pub(super) type QueryRoute = HashMap<usize, (Direction, zenoh_protocol_core::QueryTarget)>;
#[cfg(not(feature = "complete_n"))]
pub(super) type QueryRoute = Route;
pub(super) struct QueryTargetQabl {
    pub(super) direction: Direction,
    pub(super) kind: ZInt,
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
    pub(super) qabl: HashMap<ZInt, QueryableInfo>,
    pub(super) last_values: HashMap<String, (Option<DataInfo>, ZBuf)>,
}

pub(super) struct ResourceContext {
    pub(super) router_subs: HashSet<PeerId>,
    pub(super) peer_subs: HashSet<PeerId>,
    pub(super) router_qabls: HashMap<(PeerId, ZInt), QueryableInfo>,
    pub(super) peer_qabls: HashMap<(PeerId, ZInt), QueryableInfo>,
    pub(super) matches: Vec<Weak<Resource>>,
    pub(super) matching_pulls: Arc<PullCaches>,
    pub(super) routers_data_routes: Vec<Arc<Route>>,
    pub(super) peers_data_routes: Vec<Arc<Route>>,
    pub(super) client_data_route: Option<Arc<Route>>,
    pub(super) routers_query_routes: Vec<Arc<QueryTargetQablSet>>,
    pub(super) peers_query_routes: Vec<Arc<QueryTargetQablSet>>,
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
            routers_data_routes: Vec::new(),
            peers_data_routes: Vec::new(),
            client_data_route: None,
            routers_query_routes: Vec::new(),
            peers_query_routes: Vec::new(),
            client_query_route: None,
        }
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
            Some(parent) => [&parent.expr() as &str, &self.suffix].concat(),
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
            Some(ctx) => (ctx.routers_data_routes.len() > context)
                .then(|| ctx.routers_data_routes[context].clone()),
            None => None,
        }
    }

    #[inline(always)]
    pub fn peers_data_route(&self, context: usize) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => (ctx.peers_data_routes.len() > context)
                .then(|| ctx.peers_data_routes[context].clone()),
            None => None,
        }
    }

    #[inline(always)]
    pub fn client_data_route(&self) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => ctx.client_data_route.clone(),
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn routers_query_route(&self, context: usize) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => (ctx.routers_query_routes.len() > context)
                .then(|| ctx.routers_query_routes[context].clone()),
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn peers_query_route(&self, context: usize) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => (ctx.peers_query_routes.len() > context)
                .then(|| ctx.peers_query_routes[context].clone()),
            None => None,
        }
    }

    #[inline(always)]
    pub(super) fn client_query_route(&self) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => ctx.client_query_route.clone(),
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
        tables: &mut Tables,
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
                Some(res) => Resource::make_resource(tables, res, rest),
                None => {
                    let mut new = Arc::new(Resource::new(from, chunk, None));
                    if log::log_enabled!(log::Level::Debug) && rest.is_empty() {
                        log::debug!("Register resource {}", new.expr());
                    }
                    let res = Resource::make_resource(tables, &mut new, rest);
                    get_mut_unchecked(from)
                        .childs
                        .insert(String::from(chunk), new);
                    res
                }
            }
        } else {
            match from.parent.clone() {
                Some(mut parent) => {
                    Resource::make_resource(tables, &mut parent, &[&from.suffix, suffix].concat())
                }
                None => {
                    let (chunk, rest) = match suffix[1..].find('/') {
                        Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                        None => (suffix, ""),
                    };

                    match get_mut_unchecked(from).childs.get_mut(chunk) {
                        Some(res) => Resource::make_resource(tables, res, rest),
                        None => {
                            let mut new = Arc::new(Resource::new(from, chunk, None));
                            if log::log_enabled!(log::Level::Debug) && rest.is_empty() {
                                log::debug!("Register resource {}", new.expr());
                            }
                            let res = Resource::make_resource(tables, &mut new, rest);
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

    fn fst_chunk(key_expr: &str) -> (&str, &str) {
        if let Some(stripped_key_expr) = key_expr.strip_prefix('/') {
            match stripped_key_expr.find('/') {
                Some(idx) => (&key_expr[0..(idx + 1)], &key_expr[(idx + 1)..]),
                None => (key_expr, ""),
            }
        } else {
            match key_expr.find('/') {
                Some(idx) => (&key_expr[0..(idx)], &key_expr[(idx)..]),
                None => (key_expr, ""),
            }
        }
    }

    #[inline]
    pub fn decl_key(res: &Arc<Resource>, face: &mut Arc<FaceState>) -> KeyExpr<'static> {
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
                            qabl: HashMap::new(),
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
                KeyExpr {
                    scope: expr_id,
                    suffix: wildsuffix.into(),
                }
            }
            None => wildsuffix.into(),
        }
    }

    #[inline]
    pub fn get_best_key<'a>(prefix: &Arc<Resource>, suffix: &'a str, sid: usize) -> KeyExpr<'a> {
        fn get_best_key_<'a>(
            prefix: &Arc<Resource>,
            suffix: &'a str,
            sid: usize,
            checkchilds: bool,
        ) -> KeyExpr<'a> {
            if checkchilds && !suffix.is_empty() {
                let (chunk, rest) = Resource::fst_chunk(suffix);
                if let Some(child) = prefix.childs.get(chunk) {
                    return get_best_key_(child, rest, sid, true);
                }
            }
            if let Some(ctx) = prefix.session_ctxs.get(&sid) {
                if let Some(expr_id) = ctx.local_expr_id {
                    return KeyExpr {
                        scope: expr_id,
                        suffix: suffix.into(),
                    };
                } else if let Some(expr_id) = ctx.remote_expr_id {
                    return KeyExpr {
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

    pub fn get_matches(tables: &Tables, key_expr: &str) -> Vec<Weak<Resource>> {
        fn get_matches_from(
            key_expr: &str,
            is_admin: bool,
            from: &Arc<Resource>,
        ) -> Vec<Weak<Resource>> {
            let mut matches = Vec::new();
            if from.parent.is_none() {
                for child in from.childs.values() {
                    matches.append(&mut get_matches_from(key_expr, is_admin, child));
                }
                return matches;
            }
            if key_expr.is_empty() {
                if from.suffix == "/**" || from.suffix == "/" {
                    if from.context.is_some()
                        && is_admin == from.expr().starts_with(key_expr::ADMIN_PREFIX)
                    {
                        matches.push(Arc::downgrade(from));
                    }
                    for child in from.childs.values() {
                        matches.append(&mut get_matches_from(key_expr, is_admin, child));
                    }
                }
                return matches;
            }
            let (chunk, rest) = Resource::fst_chunk(key_expr);
            if key_expr::intersect(chunk, &from.suffix) {
                if rest.is_empty() || rest == "/" || rest == "/**" {
                    if from.context.is_some()
                        && is_admin == from.expr().starts_with(key_expr::ADMIN_PREFIX)
                    {
                        matches.push(Arc::downgrade(from));
                    }
                } else if chunk == "/**" || from.suffix == "/**" {
                    matches.append(&mut get_matches_from(rest, is_admin, from));
                }
                for child in from.childs.values() {
                    matches.append(&mut get_matches_from(rest, is_admin, child));
                    if chunk == "/**" || from.suffix == "/**" {
                        matches.append(&mut get_matches_from(key_expr, is_admin, child));
                    }
                }
            }
            matches
        }
        get_matches_from(
            key_expr,
            key_expr.starts_with(key_expr::ADMIN_PREFIX),
            &tables.root_res,
        )
    }

    pub fn match_resource(tables: &Tables, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            let mut matches = Resource::get_matches(tables, &res.expr());

            fn matches_contain(matches: &[Weak<Resource>], res: &Arc<Resource>) -> bool {
                for match_ in matches {
                    if Arc::ptr_eq(&match_.upgrade().unwrap(), res) {
                        return true;
                    }
                }
                false
            }

            for match_ in &mut matches {
                let mut match_ = match_.upgrade().unwrap();
                if !matches_contain(&match_.context().matches, res) {
                    get_mut_unchecked(&mut match_)
                        .context_mut()
                        .matches
                        .push(Arc::downgrade(res));
                }
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
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr_id: ZInt,
    expr: &KeyExpr,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => match face.remote_mappings.get(&expr_id) {
            Some(res) => {
                if res.expr() != format!("{}{}", prefix.expr(), expr.suffix) {
                    log::error!("Resource {} remapped. Remapping unsupported!", expr_id);
                }
            }
            None => {
                let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
                Resource::match_resource(tables, &mut res);
                let mut ctx = get_mut_unchecked(&mut res)
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| {
                        Arc::new(SessionContext {
                            face: face.clone(),
                            local_expr_id: None,
                            remote_expr_id: Some(expr_id),
                            subs: None,
                            qabl: HashMap::new(),
                            last_values: HashMap::new(),
                        })
                    })
                    .clone();

                if face.local_mappings.get(&expr_id).is_some() && ctx.local_expr_id == None {
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
                tables.compute_matches_routes(&mut res);
            }
        },
        None => log::error!("Declare resource with unknown scope {}!", expr.scope),
    }
}

pub fn unregister_expr(_tables: &mut Tables, face: &mut Arc<FaceState>, expr_id: ZInt) {
    match get_mut_unchecked(face).remote_mappings.remove(&expr_id) {
        Some(mut res) => Resource::clean(&mut res),
        None => log::error!("Undeclare unknown resource!"),
    }
}

// pub(super) struct QueryableRef {
//     pub(super) res: Arc<Resource>,
//     pub(super) kind: ZInt,
// }

// impl PartialEq for QueryableRef {
//     fn eq(&self, other: &Self) -> bool {
//         self.res.eq(&other.res)
//     }
// }

// impl Eq for QueryableRef {}

// impl Hash for QueryableRef {
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         self.res.hash(state)
//     }
// }

#[inline]
pub(super) fn elect_router<'a>(key_expr: &str, routers: &'a [PeerId]) -> &'a PeerId {
    if routers.len() == 1 {
        &routers[0]
    } else {
        routers
            .iter()
            .map(|router| {
                let mut hasher = DefaultHasher::new();
                for b in key_expr.as_bytes() {
                    hasher.write_u8(*b);
                }
                for b in router.as_slice() {
                    hasher.write_u8(*b);
                }
                (router, hasher.finish())
            })
            .max_by(|(_, s1), (_, s2)| s1.partial_cmp(s2).unwrap())
            .unwrap()
            .0
    }
}
