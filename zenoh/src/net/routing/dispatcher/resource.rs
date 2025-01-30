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
    collections::HashMap,
    convert::TryInto,
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::{key_expr::keyexpr, ExprId, WireExpr},
    network::{
        declare::{ext, queryable::ext::QueryableInfoType, Declare, DeclareBody, DeclareKeyExpr},
        interest::InterestId,
        Mapping, RequestId,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face::FaceState,
    pubsub::SubscriberInfo,
    tables::{Tables, TablesLock},
};
use crate::net::routing::{
    dispatcher::face::Face,
    router::{disable_matches_data_routes, disable_matches_query_routes},
    RoutingContext,
};

pub(crate) type NodeId = u16;

pub(crate) type Direction = (Arc<FaceState>, WireExpr<'static>, NodeId);
pub(crate) type Route = HashMap<usize, Direction>;

pub(crate) type QueryRoute = HashMap<usize, (Direction, RequestId)>;
pub(crate) struct QueryTargetQabl {
    pub(crate) direction: Direction,
    pub(crate) info: Option<QueryableInfoType>,
}
pub(crate) type QueryTargetQablSet = Vec<QueryTargetQabl>;

pub(crate) struct SessionContext {
    pub(crate) face: Arc<FaceState>,
    pub(crate) local_expr_id: Option<ExprId>,
    pub(crate) remote_expr_id: Option<ExprId>,
    pub(crate) subs: Option<SubscriberInfo>,
    pub(crate) qabl: Option<QueryableInfoType>,
    pub(crate) token: bool,
    pub(crate) in_interceptor_cache: Option<Box<dyn Any + Send + Sync>>,
    pub(crate) e_interceptor_cache: Option<Box<dyn Any + Send + Sync>>,
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
            in_interceptor_cache: None,
            e_interceptor_cache: None,
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
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) data_routes: RwLock<DataRoutes>,
    pub(crate) query_routes: RwLock<QueryRoutes>,
}

impl ResourceContext {
    fn new(hat: Box<dyn Any + Send + Sync>) -> ResourceContext {
        ResourceContext {
            hat,
            data_routes: Default::default(),
            query_routes: Default::default(),
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
    pub(crate) suffix: String,
    pub(crate) nonwild_prefix: Option<(Arc<Resource>, String)>,
    pub(crate) children: HashMap<String, Arc<Resource>>,
    pub(crate) context: Option<ResourceContext>,
    pub(crate) session_ctxs: HashMap<usize, Arc<SessionContext>>,
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
            expr: parent.expr.clone() + suffix,
            suffix: String::from(suffix),
            nonwild_prefix,
            children: HashMap::new(),
            context,
            session_ctxs: HashMap::new(),
        }
    }

    pub fn expr(&self) -> &str {
        &self.expr
    }

    pub(crate) fn key_expr(&self) -> Option<&keyexpr> {
        if !self.expr.is_empty() {
            // SAFETY: resource full expr should be a valid keyexpr
            Some(unsafe { keyexpr::from_str_unchecked(&self.expr) })
        } else {
            None
        }
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
        // SAFETY: resource expr is supposed to be a valid keyexpr
        (!self.expr.is_empty() && !other.expr.is_empty())
            && unsafe {
                keyexpr::from_str_unchecked(&self.expr)
                    .intersects(keyexpr::from_str_unchecked(&other.expr))
            }
    }

    pub fn nonwild_prefix(res: &Arc<Resource>) -> (Option<Arc<Resource>>, String) {
        match &res.nonwild_prefix {
            None => (Some(res.clone()), "".to_string()),
            Some((nonwild_prefix, wildsuffix)) => {
                if !nonwild_prefix.expr().is_empty() {
                    (Some(nonwild_prefix.clone()), wildsuffix.clone())
                } else {
                    (None, res.expr().to_string())
                }
            }
        }
    }

    pub fn root() -> Arc<Resource> {
        Arc::new(Resource {
            parent: None,
            expr: String::from(""),
            suffix: String::from(""),
            nonwild_prefix: None,
            children: HashMap::new(),
            context: None,
            session_ctxs: HashMap::new(),
        })
    }

    pub fn clean(res: &mut Arc<Resource>) {
        let mut resclone = res.clone();
        let mutres = get_mut_unchecked(&mut resclone);
        if let Some(ref mut parent) = mutres.parent {
            if Arc::strong_count(res) <= 3 && res.children.is_empty() {
                // consider only childless resource held by only one external object (+ 1 strong count for resclone, + 1 strong count for res.parent to a total of 3 )
                tracing::debug!("Unregister resource {}", res.expr());
                mutres.nonwild_prefix.take();
                {
                    get_mut_unchecked(parent).children.remove(&res.suffix);
                }
                Resource::clean(parent);
            }
        }
    }

    pub fn close(self: &mut Arc<Resource>) {
        let r = get_mut_unchecked(self);
        for c in r.children.values_mut() {
            Self::close(c);
        }
        r.parent.take();
        r.children.clear();
        r.nonwild_prefix.take();
        r.context.take();
        r.session_ctxs.clear();
    }

    #[cfg(test)]
    pub fn print_tree(from: &Arc<Resource>) -> String {
        let mut result = from.expr().to_string();
        result.push('\n');
        for child in from.children.values() {
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
            Resource::upgrade_resource(from, tables.hat_code.new_resource());
            from.clone()
        } else if let Some(stripped_suffix) = suffix.strip_prefix('/') {
            let (chunk, rest) = match stripped_suffix.find('/') {
                Some(idx) => (&suffix[0..(idx + 1)], &suffix[(idx + 1)..]),
                None => (suffix, ""),
            };

            match get_mut_unchecked(from).children.get_mut(chunk) {
                Some(res) => Resource::make_resource(tables, res, rest),
                None => {
                    let mut new = Arc::new(Resource::new(from, chunk, None));
                    if tracing::enabled!(tracing::Level::DEBUG) && rest.is_empty() {
                        tracing::debug!("Register resource {}", new.expr());
                    }
                    let res = Resource::make_resource(tables, &mut new, rest);
                    get_mut_unchecked(from)
                        .children
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

                    match get_mut_unchecked(from).children.get_mut(chunk) {
                        Some(res) => Resource::make_resource(tables, res, rest),
                        None => {
                            let mut new = Arc::new(Resource::new(from, chunk, None));
                            if tracing::enabled!(tracing::Level::DEBUG) && rest.is_empty() {
                                tracing::debug!("Register resource {}", new.expr());
                            }
                            let res = Resource::make_resource(tables, &mut new, rest);
                            get_mut_unchecked(from)
                                .children
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

            match from.children.get(chunk) {
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

                    match from.children.get(chunk) {
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
    pub fn decl_key(
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        push: bool,
    ) -> WireExpr<'static> {
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
                        Declare {
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

    pub fn get_best_key<'a>(&self, suffix: &'a str, sid: usize) -> WireExpr<'a> {
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
        fn get_best_child_key<'a>(
            prefix: &Resource,
            suffix: &'a str,
            sid: usize,
        ) -> Option<WireExpr<'a>> {
            if suffix.is_empty() {
                return None;
            }
            let (chunk, remain) = suffix.split_once('/').unwrap_or((suffix, ""));
            let child = prefix.children.get(chunk)?;
            get_best_child_key(child, remain, sid)
                .or_else(|| get_wire_expr(child, || remain.into(), sid))
        }
        fn get_best_parent_key<'a>(
            prefix: &Resource,
            suffix: &'a str,
            sid: usize,
            parent: &Resource,
        ) -> Option<WireExpr<'a>> {
            let parent_suffix = || [&prefix.expr[parent.expr.len()..], suffix].concat().into();
            get_wire_expr(parent, parent_suffix, sid)
                .or_else(|| get_best_parent_key(prefix, suffix, sid, parent.parent.as_ref()?))
        }
        get_best_child_key(self, suffix, sid)
            .or_else(|| get_wire_expr(self, || suffix.into(), sid))
            .or_else(|| get_best_parent_key(self, suffix, sid, self.parent.as_ref()?))
            .unwrap_or_else(|| [&self.expr, suffix].concat().into())
    }

    pub(crate) fn any_matches(
        root: &Arc<Resource>,
        key_expr: &keyexpr,
        mut f: impl FnMut(&Arc<Resource>) -> bool,
    ) -> bool {
        macro_rules! return_if_true {
            ($expr:expr) => {
                if $expr {
                    return true;
                }
            };
        }
        fn any_matches_trailing_wildcard(
            res: &Arc<Resource>,
            f: &mut impl FnMut(&Arc<Resource>) -> bool,
        ) -> bool {
            return_if_true!(res.context.is_some() && f(res));
            for child in res.children.values() {
                return_if_true!(any_matches_trailing_wildcard(child, f));
            }
            false
        }
        fn any_matches_rec(
            key_expr: &keyexpr,
            res: &Arc<Resource>,
            f: &mut impl FnMut(&Arc<Resource>) -> bool,
        ) -> bool {
            if res.parent.is_none() || res.suffix == "/" {
                for child in res.children.values() {
                    return_if_true!(any_matches_rec(key_expr, child, f));
                }
                return false;
            }
            let suffix: &keyexpr = res
                .suffix
                .strip_prefix('/')
                .unwrap_or(&res.suffix)
                .try_into()
                .unwrap();
            let (chunk, rest) = Resource::fst_chunk(key_expr);
            if chunk.intersects(suffix) {
                match rest {
                    None if chunk == "**" => return any_matches_trailing_wildcard(res, f),
                    None => {
                        return_if_true!(res.context.is_some() && f(res));
                        if suffix == "**" {
                            for child in res.children.values() {
                                return_if_true!(any_matches_rec(key_expr, child, f));
                            }
                        }
                        if let Some(child) =
                            res.children.get("/**").or_else(|| res.children.get("**"))
                        {
                            return_if_true!(child.context.is_some() && f(child));
                        }
                    }
                    Some(rest) if rest == "**" => {
                        return_if_true!(any_matches_trailing_wildcard(res, f))
                    }
                    Some(rest) => {
                        let recheck_keyexpr_one_level_lower = chunk == "**" || suffix == "**";
                        for child in res.children.values() {
                            return_if_true!(any_matches_rec(rest, child, f));
                            if recheck_keyexpr_one_level_lower {
                                return_if_true!(any_matches_rec(key_expr, child, f));
                            }
                        }
                        if recheck_keyexpr_one_level_lower {
                            return_if_true!(any_matches_rec(rest, res, f));
                        }
                    }
                };
            }
            false
        }
        any_matches_rec(key_expr, root, &mut f)
    }

    pub(crate) fn iter_matches(
        root: &Arc<Resource>,
        key_expr: &keyexpr,
        mut f: impl FnMut(&Arc<Resource>),
    ) {
        Self::any_matches(root, key_expr, |res| {
            f(res);
            false
        });
    }

    pub(crate) fn get_matches(root: &Arc<Resource>, key_expr: &keyexpr) -> Vec<Arc<Resource>> {
        let mut vec = Vec::new();
        Self::iter_matches(root, key_expr, |res| vec.push(res.clone()));
        vec.sort_unstable_by_key(|res| Arc::as_ptr(res));
        vec.dedup_by_key(|res| Arc::as_ptr(res));
        vec
    }

    pub fn upgrade_resource(res: &mut Arc<Resource>, hat: Box<dyn Any + Send + Sync>) {
        if res.context.is_none() {
            get_mut_unchecked(res).context = Some(ResourceContext::new(hat));
        }
    }

    pub(crate) fn get_ingress_cache(&self, face: &Face) -> Option<&Box<dyn Any + Send + Sync>> {
        self.session_ctxs
            .get(&face.state.id)
            .and_then(|ctx| ctx.in_interceptor_cache.as_ref())
    }

    pub(crate) fn get_egress_cache(&self, face: &Face) -> Option<&Box<dyn Any + Send + Sync>> {
        self.session_ctxs
            .get(&face.state.id)
            .and_then(|ctx| ctx.e_interceptor_cache.as_ref())
    }
}

pub(crate) fn register_expr(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr_id: ExprId,
    expr: &WireExpr,
) {
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
                let (mut res, mut wtables) = if res
                    .as_ref()
                    .map(|r| r.context.is_some())
                    .unwrap_or(false)
                {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr().to_string();
                    fullexpr.push_str(expr.suffix.as_ref());
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
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
    let wtables = zwrite!(tables.tables);
    match get_mut_unchecked(face).remote_mappings.remove(&expr_id) {
        Some(mut res) => Resource::clean(&mut res),
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
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    (res, wtables)
                };
                get_mut_unchecked(face)
                    .remote_key_interests
                    .insert(id, Some(res));
                drop(wtables);
            }
            None => tracing::error!(
                "Declare keyexpr interest with unknown scope {}!",
                expr.scope
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
    let wtables = zwrite!(tables.tables);
    get_mut_unchecked(face).remote_key_interests.remove(&id);
    drop(wtables);
}
