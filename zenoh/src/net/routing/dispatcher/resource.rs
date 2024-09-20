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
    convert::TryInto,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
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
use crate::net::routing::{dispatcher::face::Face, RoutingContext};

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

#[derive(Default)]
pub(crate) struct RoutesIndexes {
    pub(crate) routers: Vec<NodeId>,
    pub(crate) peers: Vec<NodeId>,
    pub(crate) clients: Vec<NodeId>,
}

#[derive(Default)]
pub(crate) struct DataRoutes {
    pub(crate) routers: Vec<Arc<Route>>,
    pub(crate) peers: Vec<Arc<Route>>,
    pub(crate) clients: Vec<Arc<Route>>,
}

impl DataRoutes {
    #[inline]
    pub(crate) fn get_route(&self, whatami: WhatAmI, context: NodeId) -> Option<Arc<Route>> {
        match whatami {
            WhatAmI::Router => (self.routers.len() > context as usize)
                .then(|| self.routers[context as usize].clone()),
            WhatAmI::Peer => {
                (self.peers.len() > context as usize).then(|| self.peers[context as usize].clone())
            }
            WhatAmI::Client => (self.clients.len() > context as usize)
                .then(|| self.clients[context as usize].clone()),
        }
    }
}

#[derive(Default)]
pub(crate) struct QueryRoutes {
    pub(crate) routers: Vec<Arc<QueryTargetQablSet>>,
    pub(crate) peers: Vec<Arc<QueryTargetQablSet>>,
    pub(crate) clients: Vec<Arc<QueryTargetQablSet>>,
}

impl QueryRoutes {
    #[inline]
    pub(crate) fn get_route(
        &self,
        whatami: WhatAmI,
        context: NodeId,
    ) -> Option<Arc<QueryTargetQablSet>> {
        match whatami {
            WhatAmI::Router => (self.routers.len() > context as usize)
                .then(|| self.routers[context as usize].clone()),
            WhatAmI::Peer => {
                (self.peers.len() > context as usize).then(|| self.peers[context as usize].clone())
            }
            WhatAmI::Client => (self.clients.len() > context as usize)
                .then(|| self.clients[context as usize].clone()),
        }
    }
}

pub(crate) struct ResourceContext {
    pub(crate) matches: Vec<Weak<Resource>>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) valid_data_routes: bool,
    pub(crate) data_routes: DataRoutes,
    pub(crate) valid_query_routes: bool,
    pub(crate) query_routes: QueryRoutes,
}

impl ResourceContext {
    fn new(hat: Box<dyn Any + Send + Sync>) -> ResourceContext {
        ResourceContext {
            matches: Vec::new(),
            hat,
            valid_data_routes: false,
            data_routes: DataRoutes::default(),
            valid_query_routes: false,
            query_routes: QueryRoutes::default(),
        }
    }

    pub(crate) fn update_data_routes(&mut self, data_routes: DataRoutes) {
        self.valid_data_routes = true;
        self.data_routes = data_routes;
    }

    pub(crate) fn disable_data_routes(&mut self) {
        self.valid_data_routes = false;
    }

    pub(crate) fn update_query_routes(&mut self, query_routes: QueryRoutes) {
        self.valid_query_routes = true;
        self.query_routes = query_routes
    }

    pub(crate) fn disable_query_routes(&mut self) {
        self.valid_query_routes = false;
    }
}

pub struct Resource {
    pub(crate) parent: Option<Arc<Resource>>,
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
            suffix: String::from(suffix),
            nonwild_prefix,
            children: HashMap::new(),
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
            Some((nonwild_prefix, wildsuffix)) => {
                if !nonwild_prefix.expr().is_empty() {
                    (Some(nonwild_prefix.clone()), wildsuffix.clone())
                } else {
                    (None, res.expr())
                }
            }
        }
    }

    #[inline]
    pub(crate) fn data_route(&self, whatami: WhatAmI, context: NodeId) -> Option<Arc<Route>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_data_routes {
                    ctx.data_routes.get_route(whatami, context)
                } else {
                    None
                }
            }

            None => None,
        }
    }

    #[inline(always)]
    pub(crate) fn query_route(
        &self,
        whatami: WhatAmI,
        context: NodeId,
    ) -> Option<Arc<QueryTargetQablSet>> {
        match &self.context {
            Some(ctx) => {
                if ctx.valid_query_routes {
                    ctx.query_routes.get_route(whatami, context)
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
        let mut result = from.expr();
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
                                wire_expr: nonwild_prefix.expr().into(),
                            }),
                        },
                        nonwild_prefix.expr(),
                    ));
                    face.update_interceptors_caches(&mut nonwild_prefix);
                    WireExpr {
                        scope: expr_id,
                        suffix: wildsuffix.into(),
                        mapping: Mapping::Sender,
                    }
                } else {
                    res.expr().into()
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
            checkclildren: bool,
        ) -> WireExpr<'a> {
            if checkclildren && !suffix.is_empty() {
                let (chunk, rest) = suffix.split_at(suffix.find('/').unwrap_or(suffix.len()));
                if let Some(child) = prefix.children.get(chunk) {
                    return get_best_key_(child, rest, sid, true);
                }
            }
            if let Some(ctx) = prefix.session_ctxs.get(&sid) {
                if let Some(expr_id) = ctx.remote_expr_id {
                    return WireExpr {
                        scope: expr_id,
                        suffix: suffix.into(),
                        mapping: Mapping::Receiver,
                    };
                } else if let Some(expr_id) = ctx.local_expr_id {
                    return WireExpr {
                        scope: expr_id,
                        suffix: suffix.into(),
                        mapping: Mapping::Sender,
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
            for child in from.children.values() {
                recursive_push(child, matches)
            }
        }
        fn get_matches_from(
            key_expr: &keyexpr,
            from: &Arc<Resource>,
            matches: &mut Vec<Weak<Resource>>,
        ) {
            if from.parent.is_none() || from.suffix == "/" {
                for child in from.children.values() {
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
                                for child in from.children.values() {
                                    get_matches_from(key_expr, child, matches)
                                }
                            }
                            if let Some(child) =
                                from.children.get("/**").or_else(|| from.children.get("**"))
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
                        for child in from.children.values() {
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
            tracing::error!("Call match_resource() on context less res {}", res.expr());
        }
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
                let mut fullexpr = prefix.expr();
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
                let ctx = get_mut_unchecked(&mut res)
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));

                get_mut_unchecked(ctx).remote_expr_id = Some(expr_id);

                get_mut_unchecked(face)
                    .remote_mappings
                    .insert(expr_id, res.clone());
                wtables.update_matches_routes(&mut res);
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
