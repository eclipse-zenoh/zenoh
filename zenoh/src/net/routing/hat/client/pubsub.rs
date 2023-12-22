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
use super::{face_hat, face_hat_mut};
use super::{HatCode, HatFace};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::pubsub::*;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::{DataRoutes, PullCaches, Route, RoutingExpr};
use crate::net::routing::dispatcher::tables::{Tables, TablesLock};
use crate::net::routing::hat::HatPubSubTrait;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLockReadGuard};
use zenoh_core::zread;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Reliability, WhatAmI, WireExpr},
    network::declare::{
        common::ext::WireExprType, ext, subscriber::ext::SubscriberInfo, Declare, DeclareBody,
        DeclareSubscriber, Mode, UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
) {
    if (src_face.id != dst_face.id || res.expr().starts_with(PREFIX_LIVELINESS))
        && !face_hat!(dst_face).local_subs.contains(res)
        && src_face.whatami == WhatAmI::Client
        || dst_face.whatami == WhatAmI::Client
    {
        face_hat_mut!(dst_face).local_subs.insert(res.clone());
        let key_expr = Resource::decl_key(res, dst_face);
        dst_face.primitives.send_declare(RoutingContext::with_expr(
            Declare {
                ext_qos: ext::QoSType::declare_default(),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::default(),
                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                    id: 0, // TODO
                    wire_expr: key_expr,
                    ext_info: *sub_info,
                }),
            },
            res.expr(),
        ));
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_subscription_to(tables, &mut dst_face, res, sub_info, src_face);
    }
}

fn register_client_subscription(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        log::debug!("Register subscription {} for {}", res.expr(), face);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => match &ctx.subs {
                Some(info) => {
                    if Mode::Pull == info.mode {
                        get_mut_unchecked(ctx).subs = Some(*sub_info);
                    }
                }
                None => {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            },
            None => {
                res.session_ctxs.insert(
                    face.id,
                    Arc::new(SessionContext {
                        face: face.clone(),
                        local_expr_id: None,
                        remote_expr_id: None,
                        subs: Some(*sub_info),
                        qabl: None,
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    face_hat_mut!(face).remote_subs.insert(res.clone());
}

fn declare_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
) {
    log::debug!("Register client subscription");
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
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

            register_client_subscription(&mut wtables, face, &mut res, sub_info);
            let mut propa_sub_info = *sub_info;
            propa_sub_info.mode = Mode::Push;

            propagate_simple_subscription(&mut wtables, &res, &propa_sub_info, face);
            // This introduced a buffer overflow on windows
            // TODO: Let's deactivate this on windows until Fixed
            #[cfg(not(windows))]
            for mcast_group in &wtables.mcast_groups {
                mcast_group
                    .primitives
                    .send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: 0, // TODO
                                wire_expr: res.expr().into(),
                                ext_info: *sub_info,
                            }),
                        },
                        res.expr(),
                    ))
            }

            disable_matches_data_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_data_routes = compute_matches_data_routes(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
            }
            drop(wtables);
        }
        None => log::error!("Declare subscription for unknown scope {}!", expr.scope),
    }
}

#[inline]
fn client_subs(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.subs.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face_hat!(face).local_subs.contains(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: 0, // TODO
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));
            face_hat_mut!(face).local_subs.remove(res);
        }
    }
}

pub(super) fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!("Unregister client subscription {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).subs = None;
    }
    face_hat_mut!(face).remote_subs.remove(res);

    let mut client_subs = client_subs(res);
    if client_subs.is_empty() {
        propagate_forget_simple_subscription(tables, res);
    }
    if client_subs.len() == 1 {
        let face = &mut client_subs[0];
        if face_hat!(face).local_subs.contains(res)
            && !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS))
        {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: 0, // TODO
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));

            face_hat_mut!(face).local_subs.remove(res);
        }
    }
}

fn forget_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
) {
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);
                undeclare_client_subscription(&mut wtables, face, &mut res);
                disable_matches_data_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_data_routes = compute_matches_data_routes(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => log::error!("Undeclare unknown subscription!"),
        },
        None => log::error!("Undeclare subscription with unknown scope!"),
    }
}

pub(super) fn pubsub_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable, // @TODO
        mode: Mode::Push,
    };
    for src_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        for sub in &face_hat!(src_face).remote_subs {
            propagate_simple_subscription_to(tables, face, sub, &sub_info, &mut src_face.clone());
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_subscription(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        sub_info: &SubscriberInfo,
        _node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        declare_client_subscription(tables, rtables, face, expr, sub_info);
    }

    fn forget_subscription(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        _node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        forget_client_subscription(tables, rtables, face, expr);
    }

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route> {
        let mut route = HashMap::new();
        let key_expr = expr.full_expr();
        if key_expr.ends_with('/') {
            return Arc::new(route);
        }
        log::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                log::warn!("Invalid KE reached the system: {}", e);
                return Arc::new(route);
            }
        };
        let res = Resource::get_resource(expr.prefix, expr.suffix);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (sid, context) in &mres.session_ctxs {
                if let Some(subinfo) = &context.subs {
                    if match tables.whatami {
                        WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                        _ => {
                            source_type == WhatAmI::Client
                                || context.face.whatami == WhatAmI::Client
                        }
                    } && subinfo.mode == Mode::Push
                    {
                        route.entry(*sid).or_insert_with(|| {
                            let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                            (context.face.clone(), key_expr.to_owned(), NodeId::default())
                        });
                    }
                }
            }
        }
        for mcast_group in &tables.mcast_groups {
            route.insert(
                mcast_group.id,
                (
                    mcast_group.clone(),
                    expr.full_expr().to_string().into(),
                    NodeId::default(),
                ),
            );
        }
        Arc::new(route)
    }

    fn compute_matching_pulls(&self, tables: &Tables, expr: &mut RoutingExpr) -> Arc<PullCaches> {
        let mut pull_caches = vec![];
        let ke = if let Ok(ke) = OwnedKeyExpr::try_from(expr.full_expr()) {
            ke
        } else {
            return Arc::new(pull_caches);
        };
        let res = Resource::get_resource(expr.prefix, expr.suffix);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &ke)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            for context in mres.session_ctxs.values() {
                if let Some(subinfo) = &context.subs {
                    if subinfo.mode == Mode::Pull {
                        pull_caches.push(context.clone());
                    }
                }
            }
        }
        Arc::new(pull_caches)
    }

    fn compute_data_routes_(
        &self,
        tables: &Tables,
        data_routes: &mut DataRoutes,
        expr: &mut RoutingExpr,
    ) {
        let route = self.compute_data_route(tables, expr, NodeId::default(), WhatAmI::Client);

        let routers_data_routes = &mut data_routes.routers;
        routers_data_routes.clear();
        routers_data_routes.resize_with(1, || Arc::new(HashMap::new()));
        routers_data_routes[0] = route.clone();

        let peers_data_routes = &mut data_routes.peers;
        peers_data_routes.clear();
        peers_data_routes.resize_with(1, || Arc::new(HashMap::new()));
        peers_data_routes[0] = route.clone();

        let clients_data_routes = &mut data_routes.clients;
        clients_data_routes.clear();
        clients_data_routes.resize_with(1, || Arc::new(HashMap::new()));
        clients_data_routes[0] = route;
    }
}
