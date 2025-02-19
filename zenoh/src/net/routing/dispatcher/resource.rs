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
    collections::HashMap,
    convert::TryInto,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    sync::{Arc, OnceLock, RwLock, Weak},
};

use zenoh_collections::{CompactMap, SingleOrBoxHashSet};
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
    pub(crate) matches: Vec<Weak<Resource>>,      // 24
    pub(crate) hat: Box<dyn Any + Send + Sync>,   // 8
    pub(crate) data_routes: RwLock<DataRoutes>,   // 96 = 16 + 80
    pub(crate) query_routes: RwLock<QueryRoutes>, // 96 = 16 + 80
}

impl ResourceContext {
    fn new(hat: Box<dyn Any + Send + Sync>) -> ResourceContext {
        ResourceContext {
            matches: Vec::new(),
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
    pub(crate) parent: Option<Arc<Resource>>, // 8
    pub(crate) suffix: Box<str>,              // 16
    pub(crate) lazy_expr: OnceLock<Box<str>>, // 24
    pub(crate) nonwild_prefix: Option<(Arc<Resource>, usize)>, // 16
    pub(crate) children: SingleOrBoxHashSet<Child>, // 16
    pub(crate) context: Option<Box<ResourceContext>>, // 8
    pub(crate) session_ctxs: CompactMap<usize, Arc<SessionContext>>, // 24
} // 112

impl PartialEq for Resource {
    fn eq(&self, other: &Self) -> bool {
        if self.suffix != other.suffix {
            return false;
        }

        match (&self.parent, &other.parent) {
            (None, None) => true,
            (None, Some(_)) | (Some(_), None) => false,
            (Some(lhs), Some(rhs)) => lhs == rhs,
        }
    }
}
impl Eq for Resource {}

// NOTE: The `clippy::mutable_key_type` lint takes issue with the fact that `Resource` contains
// interior mutable data. A configuration option is used to assert that the accessed fields are
// not interior mutable in clippy.toml. Thus care should be taken to ensure soundness of this impl
// as Clippy will not warn about its usage in sets/maps.
impl Hash for Resource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.suffix.hash(state);
        if let Some(r) = &self.parent {
            r.hash(state);
        }
    }
}

pub struct Resources {
    res: Option<Arc<Resource>>,
}

impl<'a> Iterator for Resources {
    type Item = Arc<Resource>;

    fn next(&mut self) -> Option<Self::Item> {
        let src = self.res.as_ref().and_then(|r| r.parent.clone());
        std::mem::replace(&mut self.res, src)
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
        self.0.suffix == other.0.suffix
    }
}

impl Eq for Child {}

impl Hash for Child {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.suffix.hash(state);
    }
}

impl Borrow<str> for Child {
    fn borrow(&self) -> &str {
        &self.0.suffix
    }
}

impl Resource {
    fn new(
        parent: &Arc<Resource>,
        suffix: &str,
        context: Option<Box<ResourceContext>>,
    ) -> Resource {
        let nonwild_prefix = match &parent.nonwild_prefix {
            None => {
                if suffix.contains('*') {
                    Some((parent.clone(), parent.expr_len()))
                } else {
                    None
                }
            }
            Some((prefix, wildsuffix)) => Some((prefix.clone(), *wildsuffix)),
        };

        Resource {
            parent: Some(parent.clone()),
            lazy_expr: OnceLock::new(),
            suffix: Box::from(suffix),
            nonwild_prefix,
            children: SingleOrBoxHashSet::new(),
            context,
            session_ctxs: CompactMap::new(),
        }
    }

    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    pub fn branch(res: &Arc<Resource>) -> Resources {
        Resources {
            res: Some(res.clone()),
        }
    }

    #[track_caller]
    pub fn expr(&self) -> &str {
        let was_none = self.lazy_expr.get().is_none();

        let expr = self.lazy_expr.get_or_init(|| {
            if let Some(r) = self.parent.as_ref() {
                let mut s = String::with_capacity(r.expr_len());
                for r1 in Resource::branch(r).collect::<Vec<_>>().into_iter().rev() {
                    s.push_str(&r1.suffix);
                }
                s.push_str(&self.suffix);
                s.into_boxed_str()
            } else {
                Box::from(self.suffix())
            }
        });

        if was_none {
            eprintln!(
                "Evaluated expr '{}' from {}",
                &expr,
                std::panic::Location::caller()
            );
        }

        expr
    }

    pub fn expr_len(&self) -> usize {
        self.suffix.len() + self.parent.as_ref().map(|r| r.expr_len()).unwrap_or(0)
    }

    #[track_caller]
    pub(crate) fn context(&self) -> &ResourceContext {
        if self.context.is_none() {
            panic!(
                "Attempted to access null context on resource '{}' from {}",
                self.expr(),
                std::panic::Location::caller()
            )
        }

        self.context.as_ref().unwrap()
    }

    #[track_caller]
    pub(crate) fn context_mut(&mut self) -> &mut ResourceContext {
        if self.context.is_none() {
            panic!(
                "Attempted to mutably access null context on resource '{}' from {}",
                self.expr(),
                std::panic::Location::caller()
            )
        }

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

    pub fn nonwild_prefix(res: &Arc<Resource>) -> (Option<Arc<Resource>>, &str) {
        match &res.nonwild_prefix {
            None => (Some(res.clone()), ""),
            Some((nonwild_prefix, wildsuffix)) => {
                if nonwild_prefix.expr_len() != 0 {
                    (Some(nonwild_prefix.clone()), &res.expr()[*wildsuffix..])
                } else {
                    (None, res.expr())
                }
            }
        }
    }

    pub fn root() -> Arc<Resource> {
        Arc::new(Resource {
            parent: None,
            suffix: Box::from(""),
            lazy_expr: OnceLock::new(),
            nonwild_prefix: None,
            children: SingleOrBoxHashSet::new(),
            context: None,
            session_ctxs: CompactMap::new(),
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
                    get_mut_unchecked(parent).children.remove(&*res.suffix);
                }
                Resource::clean(parent);
            }
        }
    }

    pub fn close(self: &mut Arc<Resource>) {
        if self.parent.is_none() {
            eprintln!("{}", Resource::print_tree(self));
        }

        let r = get_mut_unchecked(self);
        for mut c in r.children.drain() {
            Self::close(&mut c);
        }
        r.parent.take();
        r.nonwild_prefix.take();
        r.context.take();
    }

    pub fn print_tree(from: &Arc<Resource>) -> String {
        let mut result = format!(
            "(is_expr_init={}, is_context_none={}, is_nonwild_prefix_none={}, len(session_ctxs)={}, len(children)={}, expr={})",
            from.lazy_expr.get().is_some(),
            from.context.is_none(),
            from.nonwild_prefix.is_none(),
            from.session_ctxs.len(),
            from.children.len(),
            from.expr()
        );
        result.push('\n');
        for child in from.children.iter() {
            result.push_str(&Resource::print_tree(child));
        }

        result.push('\n');
        result.push_str(&format!(
            "Resource struct size: {}\n",
            std::mem::size_of::<Resource>()
        ));
        result.push_str(&format!(
            "ResourceContext struct size: {}\n",
            std::mem::size_of::<ResourceContext>()
        ));
        result
    }

    pub fn make_resource(
        tables: &mut Tables,
        from: &mut Arc<Resource>,
        suffix: &str,
    ) -> Arc<Resource> {
        let Some((chunk, rest)) = Self::split_first_chunk(suffix) else {
            Resource::upgrade_resource(from, tables.hat_code.new_resource());
            return from.clone();
        };
        if !chunk.starts_with('/') {
            if let Some(parent) = &mut from.parent.clone() {
                return Resource::make_resource(tables, parent, &[&from.suffix(), suffix].concat());
            }
        }
        if let Some(child) = get_mut_unchecked(from).children.get(chunk) {
            return Resource::make_resource(tables, &mut child.0.clone(), rest);
        }
        let mut new = Arc::new(Resource::new(from, chunk, None));
        if rest.is_empty() {
            tracing::debug!("Register resource {}", new.expr());
        }
        let res = Resource::make_resource(tables, &mut new, rest);
        get_mut_unchecked(from).children.insert(Child(new));
        res
    }

    #[inline]
    pub fn get_resource(from: &Arc<Resource>, suffix: &str) -> Option<Arc<Resource>> {
        let Some((chunk, rest)) = Self::split_first_chunk(suffix) else {
            return Some(from.clone());
        };
        if !chunk.starts_with('/') {
            if let Some(parent) = &from.parent {
                return Resource::get_resource(parent, &[&from.suffix(), suffix].concat());
            }
        }
        Resource::get_resource(from.children.get(chunk)?, rest)
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
        let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(res);
        match nonwild_prefix {
            Some(mut nonwild_prefix) => {
                // zenoh/demos/**
                if let Some(ctx) = get_mut_unchecked(&mut nonwild_prefix)
                    .session_ctxs
                    .get(&face.id)
                {
                    if let Some(expr_id) = ctx.remote_expr_id {
                        return WireExpr {
                            scope: expr_id,
                            suffix: wildsuffix.to_string().into(),
                            mapping: Mapping::Receiver,
                        };
                    }
                    if let Some(expr_id) = ctx.local_expr_id {
                        return WireExpr {
                            scope: expr_id,
                            suffix: wildsuffix.to_string().into(),
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
                        suffix: wildsuffix.to_string().into(),
                        mapping: Mapping::Sender,
                    }
                } else {
                    res.expr().to_string().into()
                }
            }
            None => wildsuffix.to_string().into(),
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
            prefix: &Resource,
            suffix: &'a str,
            sid: usize,
        ) -> Option<WireExpr<'a>> {
            let (chunk, rest) = Resource::split_first_chunk(suffix)?;
            let child = prefix.children.get(chunk)?;
            get_best_child_key(child, rest, sid)
                .or_else(|| get_wire_expr(child, || rest.into(), sid))
        }
        /// Walk through the parent tree, looking for a declared keyexpr.
        fn get_best_parent_key<'a>(
            prefix: &Resource,
            suffix: &'a str,
            sid: usize,
            parent: &Resource,
        ) -> Option<WireExpr<'a>> {
            let parent_suffix = || {
                [&prefix.expr()[parent.expr_len()..], suffix]
                    .concat()
                    .into()
            };
            get_wire_expr(parent, parent_suffix, sid)
                .or_else(|| get_best_parent_key(prefix, suffix, sid, parent.parent.as_ref()?))
        }
        get_best_child_key(self, suffix, sid)
            .or_else(|| get_wire_expr(self, || suffix.into(), sid))
            .or_else(|| get_best_parent_key(self, suffix, sid, self.parent.as_ref()?))
            .unwrap_or_else(|| [&self.expr(), suffix].concat().into())
    }

    pub fn get_matches(tables: &Tables, key_expr: &keyexpr) -> Vec<Weak<Resource>> {
        fn recursive_push(from: &Arc<Resource>, matches: &mut Vec<Weak<Resource>>) {
            if from.context.is_some() {
                matches.push(Arc::downgrade(from));
            }
            for child in from.children.iter() {
                recursive_push(child, matches)
            }
        }
        fn get_matches_from(
            key_expr: &keyexpr,
            from: &Arc<Resource>,
            matches: &mut Vec<Weak<Resource>>,
        ) {
            if from.parent.is_none() || &*from.suffix == "/" {
                for child in from.children.iter() {
                    get_matches_from(key_expr, child, matches);
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
            if ke_chunk.intersects(suffix) {
                match ke_rest {
                    None => {
                        if ke_chunk.as_bytes() == b"**" {
                            recursive_push(from, matches)
                        } else {
                            if from.context.is_some() {
                                matches.push(Arc::downgrade(from));
                            }
                            if suffix.as_bytes() == b"**" {
                                for child in from.children.iter() {
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
                            ke_chunk.as_bytes() == b"**" || suffix.as_bytes() == b"**";
                        for child in from.children.iter() {
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
            get_mut_unchecked(res).context = Some(Box::new(ResourceContext::new(hat)));
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
