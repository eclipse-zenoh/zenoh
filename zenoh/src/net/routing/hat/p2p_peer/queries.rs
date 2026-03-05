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
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::{
        key_expr::include::{Includer, DEFAULT_INCLUDER},
        WhatAmI,
    },
    network::{
        declare::{
            common::ext::WireExprType, ext, queryable::ext::QueryableInfoType, Declare,
            DeclareBody, DeclareQueryable, QueryableId, UndeclareQueryable,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            local_resources::LocalResourceInfoTrait,
            resource::{NodeId, Resource, SessionContext},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, Tables},
        },
        hat::{
            p2p_peer::{initial_interest, INITIAL_INTEREST_ID},
            CurrentFutureTrait, HatQueriesTrait, SendDeclare, Sources,
        },
        router::{get_remote_qabl_info, merge_qabl_infos, update_queryable_info},
        RoutingContext,
    },
};

#[inline]
fn local_qabl_info(
    _tables: &Tables,
    res: &Arc<Resource>,
    face: &Arc<FaceState>,
) -> QueryableInfoType {
    res.session_ctxs
        .values()
        .fold(None, |accu, ctx| {
            if ctx.face.id != face.id {
                if let Some(info) = ctx.qabl.as_ref() {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
                    })
                } else {
                    accu
                }
            } else {
                accu
            }
        })
        .unwrap_or(QueryableInfoType::DEFAULT)
}

#[inline]
fn maybe_register_local_queryable(
    tables: &Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    initial_interest: Option<InterestId>,
    send_declare: &mut SendDeclare,
) {
    let (should_notify, simple_interests) = match initial_interest {
        Some(interest) => (true, HashSet::from([interest])),
        None => face_hat!(dst_face)
            .remote_interests
            .iter()
            .filter(|(_, i)| i.options.queryables() && i.matches(res))
            .fold(
                (false, HashSet::new()),
                |(_, mut simple_interests), (id, i)| {
                    if !i.options.aggregate() {
                        simple_interests.insert(*id);
                    }
                    (true, simple_interests)
                },
            ),
    };

    if !should_notify {
        return;
    }

    let new_info = local_qabl_info(tables, res, dst_face);
    let face_hat_mut = face_hat_mut!(dst_face);
    let (_, qabls_to_notify) = face_hat_mut.local_qabls.insert_simple_resource(
        res.clone(),
        new_info,
        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
        simple_interests,
    );

    for update in qabls_to_notify {
        let key_expr = Resource::decl_key(
            &update.resource,
            dst_face,
            super::push_declaration_profile(dst_face),
        );
        send_declare(
            &dst_face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                        id: update.id,
                        wire_expr: key_expr.clone(),
                        ext_info: update.info,
                    }),
                },
                update.resource.expr().to_string(),
            ),
        );
    }
}

#[inline]
fn maybe_unregister_local_queryable(
    face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for update in face_hat_mut!(face).local_qabls.remove_simple_resource(res) {
        match update.update {
            Some(new_qabl_info) => {
                let key_expr = Resource::decl_key(
                    &update.resource,
                    face,
                    super::push_declaration_profile(face),
                );
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: update.id,
                                wire_expr: key_expr.clone(),
                                ext_info: new_qabl_info,
                            }),
                        },
                        update.resource.expr().to_string(),
                    ),
                );
            }
            None => send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id: update.id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    update.resource.expr().to_string(),
                ),
            ),
        };
    }
}

#[inline]
fn propagate_simple_queryable_to(
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &Option<&mut Arc<FaceState>>,
    send_declare: &mut SendDeclare,
) {
    if src_face
        .as_ref()
        .map(|src_face| {
            dst_face.id != src_face.id
                && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
        })
        .unwrap_or(true)
    {
        if dst_face.whatami != WhatAmI::Client {
            maybe_register_local_queryable(
                tables,
                dst_face,
                res,
                Some(INITIAL_INTEREST_ID),
                send_declare,
            );
        } else {
            maybe_register_local_queryable(tables, dst_face, res, None, send_declare);
        }
    }
}

fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: Option<&mut Arc<FaceState>>,
    send_declare: &mut SendDeclare,
) {
    let faces = tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>();
    for mut dst_face in faces {
        propagate_simple_queryable_to(tables, &mut dst_face, res, &src_face, send_declare);
    }
}

fn register_simple_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        get_mut_unchecked(
            res.session_ctxs
                .entry(face.id)
                .or_insert_with(|| Arc::new(SessionContext::new(face.clone()))),
        )
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face)
        .remote_qabls
        .insert(id, (res.clone(), *qabl_info));
}

fn declare_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
    send_declare: &mut SendDeclare,
) {
    register_simple_queryable(tables, face, id, res, qabl_info);
    propagate_simple_queryable(tables, res, Some(face), send_declare);
}

#[inline]
fn simple_qabls(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.qabl.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

fn propagate_forget_simple_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
        maybe_unregister_local_queryable(face, res, send_declare);
    }
}

pub(super) fn undeclare_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    let remote_qabl_info = get_remote_qabl_info(&face_hat_mut!(face).remote_qabls, res);

    if update_queryable_info(res, face.id, &remote_qabl_info) {
        let mut simple_qabls = simple_qabls(res);
        if simple_qabls.is_empty() {
            propagate_forget_simple_queryable(tables, res, send_declare);
        } else {
            propagate_simple_queryable(tables, res, None, send_declare);
        }
        if simple_qabls.len() == 1 {
            maybe_unregister_local_queryable(&mut simple_qabls[0], res, send_declare);
        }
    }
}

fn forget_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some((mut res, _)) = face_hat_mut!(face).remote_qabls.remove(&id) {
        undeclare_simple_queryable(tables, face, &mut res, send_declare);
        Some(res)
    } else {
        None
    }
}

pub(super) fn queries_new_face(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if face.whatami != WhatAmI::Client {
        for src_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for (ref qabl, _) in face_hat!(src_face).remote_qabls.values() {
                propagate_simple_queryable_to(
                    tables,
                    face,
                    qabl,
                    &Some(&mut src_face.clone()),
                    send_declare,
                );
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

fn get_queryables_matching_resource<'a>(
    tables: &'a Tables,
    face: &Arc<FaceState>,
    res: Option<&'a Arc<Resource>>,
) -> impl Iterator<Item = &'a Arc<Resource>> {
    let face_id = face.id;

    tables
        .faces
        .values()
        .filter(move |f| f.id != face_id)
        .flat_map(|f| face_hat!(f).remote_qabls.values())
        .map(|(qabl, _)| qabl)
        .filter(move |qabl| qabl.context.is_some() && res.map(|r| qabl.matches(r)).unwrap_or(true))
}

pub(super) fn declare_qabl_interest(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    interest_id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    if face.whatami != WhatAmI::Client {
        return;
    }
    let res = res.map(|r| r.clone());
    let matching_qabls = get_queryables_matching_resource(tables, face, res.as_ref());

    if aggregate && (mode.current() || mode.future()) {
        if let Some(aggregated_res) = &res {
            let (resource_id, qabl_info) = if mode.future() {
                for qabl in matching_qabls {
                    let qabl_info = local_qabl_info(tables, qabl, face);
                    let face_hat_mut = face_hat_mut!(face);
                    face_hat_mut.local_qabls.insert_simple_resource(
                        qabl.clone(),
                        qabl_info,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::new(),
                    );
                }
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut.local_qabls.insert_aggregated_resource(
                    aggregated_res.clone(),
                    || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                    HashSet::from_iter([interest_id]),
                )
            } else {
                (
                    0,
                    QueryableInfoType::aggregate_many(
                        aggregated_res,
                        matching_qabls.map(|q| (q, local_qabl_info(tables, q, face))),
                    ),
                )
            };
            if let Some(ext_info) = mode.current().then_some(qabl_info).flatten() {
                // send declare only if there is at least one resource matching the aggregate
                let wire_expr =
                    Resource::decl_key(aggregated_res, face, super::push_declaration_profile(face));
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest_id),
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: resource_id,
                                wire_expr,
                                ext_info,
                            }),
                        },
                        aggregated_res.expr().to_string(),
                    ),
                );
            }
        }
    } else if !aggregate && mode.current() {
        for qabl in matching_qabls {
            let qabl_info = local_qabl_info(tables, qabl, face);
            let resource_id = if mode.future() {
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut
                    .local_qabls
                    .insert_simple_resource(
                        qabl.clone(),
                        qabl_info,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from([interest_id]),
                    )
                    .0
            } else {
                0
            };
            let wire_expr = Resource::decl_key(qabl, face, super::push_declaration_profile(face));
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(interest_id),
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                            id: resource_id,
                            wire_expr,
                            ext_info: qabl_info,
                        }),
                    },
                    qabl.expr().to_string(),
                ),
            );
        }
    }
}

impl HatQueriesTrait for HatCode {
    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        declare_simple_queryable(tables, face, id, res, qabl_info, send_declare);
    }

    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        forget_simple_queryable(tables, face, id, send_declare)
    }

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for face in tables.faces.values() {
            for (ref qabl, _) in face_hat!(face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                let whatami = if face.is_local {
                    tables.whatami
                } else {
                    face.whatami
                };
                match whatami {
                    WhatAmI::Router => srcs.routers.push(face.zid),
                    WhatAmI::Peer => srcs.peers.push(face.zid),
                    WhatAmI::Client => srcs.clients.push(face.zid),
                }
            }
        }
        Vec::from_iter(qabls)
    }

    fn get_queriers(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face).remote_interests.values() {
                if interest.options.queryables() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        let whatami = if face.is_local {
                            tables.whatami
                        } else {
                            face.whatami
                        };
                        match whatami {
                            WhatAmI::Router => sources.routers.push(face.zid),
                            WhatAmI::Peer => sources.peers.push(face.zid),
                            WhatAmI::Client => sources.clients.push(face.zid),
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
    }

    fn compute_query_route(
        &self,
        tables: &Tables,
        expr: &RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<QueryTargetQablSet> {
        let mut route = QueryTargetQablSet::new();
        let Some(key_expr) = expr.key_expr() else {
            return EMPTY_ROUTE.clone();
        };
        tracing::trace!(
            "compute_query_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
            for face_ctx @ (_, ctx) in &mres.session_ctxs {
                if source_type == WhatAmI::Client || ctx.face.whatami == WhatAmI::Client {
                    if let Some(qabl) = QueryTargetQabl::new(face_ctx, expr, complete) {
                        route.push(qabl);
                    }
                }
            }
        }

        if source_type == WhatAmI::Client {
            // TODO: BestMatching: What if there is a local compete ?
            for face in tables.faces.values() {
                if face.whatami == WhatAmI::Router {
                    let has_interest_finalized = expr
                        .resource()
                        .and_then(|res| res.session_ctxs.get(&face.id))
                        .is_some_and(|ctx| ctx.queryable_interest_finalized);
                    if !has_interest_finalized {
                        let wire_expr = expr.get_best_key(face.id);
                        route.push(QueryTargetQabl {
                            direction: (face.clone(), wire_expr.to_owned(), NodeId::default()),
                            info: None,
                        });
                    }
                } else if face.whatami == WhatAmI::Peer
                    && initial_interest(face).is_some_and(|i| !i.finalized)
                {
                    let wire_expr = expr.get_best_key(face.id);
                    route.push(QueryTargetQabl {
                        direction: (face.clone(), wire_expr.to_owned(), NodeId::default()),
                        info: None,
                    });
                }
            }
        }

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    fn get_matching_queryables(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_queryables = HashMap::new();
        tracing::trace!(
            "get_matching_queryables({}; complete: {})",
            key_expr,
            complete
        );
        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            if complete && !KeyExpr::keyexpr_include(mres.expr(), key_expr) {
                continue;
            }
            for (sid, context) in &mres.session_ctxs {
                if match complete {
                    true => context.qabl.is_some_and(|q| q.complete),
                    false => context.qabl.is_some(),
                } {
                    matching_queryables
                        .entry(*sid)
                        .or_insert_with(|| context.face.clone());
                }
            }
        }
        matching_queryables
    }
}
