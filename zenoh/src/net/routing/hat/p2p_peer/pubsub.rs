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
    collections::HashMap,
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, WhatAmI},
    network::{
        declare::{
            common::ext::WireExprType, ext, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
            UndeclareSubscriber,
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
            face::{Face, FaceState},
            interests::RemoteInterest,
            pubsub::SubscriberInfo,
            resource::{NodeId, Resource, SessionContext},
            tables::{Route, RoutingExpr, Tables},
        },
        hat::{
            p2p_peer::initial_interest, CurrentFutureTrait, HatPubSubTrait, SendDeclare, Sources,
        },
    },
};

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Face,
    res: &Arc<Resource>,
    _sub_info: &SubscriberInfo,
    src_face: &Face,
    send_declare: &mut SendDeclare,
) {
    if (src_face.state.id != dst_face.state.id)
        && !face_hat!(dst_face.state).local_subs.contains_key(res)
        && (src_face.state.whatami == WhatAmI::Client || dst_face.state.whatami == WhatAmI::Client)
    {
        if dst_face.state.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face.state)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(&mut dst_face.state)
                .local_subs
                .insert(res.clone(), id);
            let key_expr = Resource::decl_key(
                res,
                dst_face,
                super::push_declaration_profile(&dst_face.state),
            );
            send_declare(
                &dst_face.state,
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id,
                        wire_expr: key_expr,
                    }),
                },
                Some(res.clone()),
            );
        } else {
            let matching_interests = face_hat!(dst_face.state)
                .remote_interests
                .values()
                .filter(|i| i.options.subscribers() && i.matches(res))
                .cloned()
                .collect::<Vec<_>>();

            for RemoteInterest {
                res: int_res,
                options,
                ..
            } in matching_interests
            {
                let res = if options.aggregate() {
                    int_res.as_ref().unwrap_or(res)
                } else {
                    res
                };
                if !face_hat!(dst_face.state).local_subs.contains_key(res) {
                    let id = face_hat!(dst_face.state)
                        .next_id
                        .fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(&mut dst_face.state)
                        .local_subs
                        .insert(res.clone(), id);
                    let key_expr = Resource::decl_key(
                        res,
                        dst_face,
                        super::push_declaration_profile(&dst_face.state),
                    );
                    send_declare(
                        &dst_face.state,
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr: key_expr,
                            }),
                        },
                        Some(res.clone()),
                    );
                }
            }
        }
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &Face,
    send_declare: &mut SendDeclare,
) {
    for mut dst_face in tables.faces.values().cloned().collect::<Vec<_>>() {
        propagate_simple_subscription_to(
            tables,
            &mut dst_face,
            res,
            sub_info,
            src_face,
            send_declare,
        );
    }
}

fn register_simple_subscription(
    _tables: &mut Tables,
    face: &Face,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.state.id) {
            Some(ctx) => {
                if ctx.subs.is_none() {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.state.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).subs = Some(*sub_info);
            }
        }
    }
    face_hat_mut!(&mut face.state.clone())
        .remote_subs
        .insert(id, res.clone());
}

fn declare_simple_subscription(
    tables: &mut Tables,
    face: &Face,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    send_declare: &mut SendDeclare,
) {
    register_simple_subscription(tables, face, id, res, sub_info);

    propagate_simple_subscription(tables, res, sub_info, face, send_declare);
    // This introduced a buffer overflow on windows
    // TODO: Let's deactivate this on windows until Fixed
    #[cfg(not(windows))]
    if face.state.whatami == WhatAmI::Client {
        for mcast_group in &tables.mcast_groups {
            if mcast_group.state.mcast_group != face.state.mcast_group {
                mcast_group.state.intercept_declare(
                    &mut Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id: 0, // @TODO use proper SubscriberId
                            wire_expr: res.expr().to_string().into(),
                        }),
                    },
                    Some(res),
                )
            }
        }
    }
}

#[inline]
fn simple_subs(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.subs.is_some() {
                Some(ctx.face.state.clone())
            } else {
                None
            }
        })
        .collect()
}

#[inline]
fn remote_simple_subs(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.state.id != face.id && ctx.subs.is_some())
}

fn propagate_forget_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(res) {
            send_declare(
                &face.state,
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                },
                Some(res.clone()),
            );
        }
        for res in face_hat!(face.state)
            .local_subs
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade()
                    .is_some_and(|m| m.context.is_some() && remote_simple_subs(&m, &face.state))
            }) {
                if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(&res) {
                    send_declare(
                        &face.state,
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        Some(res.clone()),
                    );
                }
            }
        }
    }
}

pub(super) fn undeclare_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    if !face_hat_mut!(face).remote_subs.values().any(|s| *s == *res) {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).subs = None;
        }

        let mut simple_subs = simple_subs(res);
        if simple_subs.is_empty() {
            propagate_forget_simple_subscription(tables, res, send_declare);
        }

        if simple_subs.len() == 1 {
            let mut face = &mut simple_subs[0];
            if let Some(id) = face_hat_mut!(face).local_subs.remove(res) {
                send_declare(
                    face,
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                            id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    Some(res.clone()),
                );
            }
            for res in face_hat!(face)
                .local_subs
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade()
                        .is_some_and(|m| m.context.is_some() && remote_simple_subs(&m, face))
                }) {
                    if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(&res) {
                        send_declare(
                            face,
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
                                }),
                            },
                            Some(res.clone()),
                        );
                    }
                }
            }
        }
    }
}

fn forget_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_subs.remove(&id) {
        undeclare_simple_subscription(tables, face, &mut res, send_declare);
        Some(res)
    } else {
        None
    }
}

pub(super) fn pubsub_new_face(
    tables: &mut Tables,
    face: &mut Face,
    send_declare: &mut SendDeclare,
) {
    if face.state.whatami != WhatAmI::Client {
        let sub_info = SubscriberInfo;
        for src_face in tables.faces.values().cloned().collect::<Vec<_>>() {
            for sub in face_hat!(src_face.state).remote_subs.values() {
                propagate_simple_subscription_to(
                    tables,
                    face,
                    sub,
                    &sub_info,
                    &src_face,
                    send_declare,
                );
            }
        }
    }
}

#[inline]
fn make_sub_id(res: &Arc<Resource>, face: &mut Arc<FaceState>, mode: InterestMode) -> u32 {
    if mode.future() {
        if let Some(id) = face_hat!(face).local_subs.get(res) {
            *id
        } else {
            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(face).local_subs.insert(res.clone(), id);
            id
        }
    } else {
        0
    }
}

pub(super) fn declare_sub_interest(
    tables: &mut Tables,
    face: &Face,
    id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    if mode.current() && face.state.whatami == WhatAmI::Client {
        let interest_id = Some(id);
        if let Some(res) = res.as_ref() {
            if aggregate {
                if tables.faces.values().any(|src_face| {
                    src_face.state.id != face.state.id
                        && face_hat!(src_face.state)
                            .remote_subs
                            .values()
                            .any(|sub| sub.context.is_some() && sub.matches(res))
                }) {
                    let id = make_sub_id(res, &mut face.state.clone(), mode);
                    let wire_expr =
                        Resource::decl_key(res, face, super::push_declaration_profile(&face.state));
                    send_declare(
                        &face.state,
                        Declare {
                            interest_id,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr,
                            }),
                        },
                        Some(res.deref().clone()),
                    );
                }
            } else {
                for src_face in tables.faces.values().cloned().collect::<Vec<_>>() {
                    if src_face.state.id != face.state.id {
                        for sub in face_hat!(src_face.state).remote_subs.values() {
                            if sub.context.is_some() && sub.matches(res) {
                                let id = make_sub_id(sub, &mut face.state.clone(), mode);
                                let wire_expr = Resource::decl_key(
                                    sub,
                                    face,
                                    super::push_declaration_profile(&face.state),
                                );
                                send_declare(
                                    &face.state,
                                    Declare {
                                        interest_id,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                            id,
                                            wire_expr,
                                        }),
                                    },
                                    Some(sub.clone()),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            for src_face in tables.faces.values().cloned().collect::<Vec<_>>() {
                if src_face.state.id != face.state.id {
                    for sub in face_hat!(src_face.state).remote_subs.values() {
                        let id = make_sub_id(sub, &mut face.state.clone(), mode);
                        let wire_expr = Resource::decl_key(
                            sub,
                            face,
                            super::push_declaration_profile(&face.state),
                        );
                        send_declare(
                            &face.state,
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id,
                                    wire_expr,
                                }),
                            },
                            Some(sub.clone()),
                        );
                    }
                }
            }
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &Face,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        declare_simple_subscription(tables, face, id, res, sub_info, send_declare);
    }

    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        forget_simple_subscription(tables, face, id, send_declare)
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known suscriptions (keys)
        let mut subs = HashMap::new();
        for src_face in tables.faces.values() {
            for sub in face_hat!(src_face.state).remote_subs.values() {
                // Insert the key in the list of known suscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a suscription source in the proper list
                match src_face.state.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.state.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.state.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.state.zid),
                }
            }
        }
        Vec::from_iter(subs)
    }

    fn get_publications(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face.state).remote_interests.values() {
                if interest.options.subscribers() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        match face.state.whatami {
                            WhatAmI::Router => sources.routers.push(face.state.zid),
                            WhatAmI::Peer => sources.peers.push(face.state.zid),
                            WhatAmI::Client => sources.clients.push(face.state.zid),
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
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
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                tracing::warn!("Invalid KE reached the system: {}", e);
                return Arc::new(route);
            }
        };

        if source_type == WhatAmI::Client {
            for face in tables
                .faces
                .values()
                .filter(|f| f.state.whatami == WhatAmI::Router)
            {
                if !face.state.local_interests.values().any(|interest| {
                    interest.finalized
                        && interest.options.subscribers()
                        && interest
                            .res
                            .as_ref()
                            .map(|res| KeyExpr::keyexpr_include(res.expr(), expr.full_expr()))
                            .unwrap_or(true)
                }) || face_hat!(face.state)
                    .remote_subs
                    .values()
                    .any(|sub| KeyExpr::keyexpr_intersect(sub.expr(), expr.full_expr()))
                {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.state.id);
                    route.insert(
                        face.state.id,
                        (face.clone(), key_expr.to_owned(), NodeId::default()),
                    );
                }
            }

            for face in tables.faces.values().filter(|f| {
                f.state.whatami == WhatAmI::Peer
                    && !initial_interest(&f.state)
                        .map(|i| i.finalized)
                        .unwrap_or(true)
            }) {
                route.entry(face.state.id).or_insert_with(|| {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.state.id);
                    (face.clone(), key_expr.to_owned(), NodeId::default())
                });
            }
        }

        let res = Resource::get_resource(expr.prefix, expr.suffix);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some()
                    && (source_type == WhatAmI::Client
                        || context.face.state.whatami == WhatAmI::Client)
                {
                    route.entry(*sid).or_insert_with(|| {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                        (context.face.clone(), key_expr.to_owned(), NodeId::default())
                    });
                }
            }
        }
        for mcast_group in &tables.mcast_groups {
            route.insert(
                mcast_group.state.id,
                (
                    mcast_group.clone(),
                    expr.full_expr().to_string().into(),
                    NodeId::default(),
                ),
            );
        }
        Arc::new(route)
    }

    #[zenoh_macros::unstable]
    fn get_matching_subscriptions(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_subscriptions = HashMap::new();
        if key_expr.ends_with('/') {
            return matching_subscriptions;
        }
        tracing::trace!("get_matching_subscriptions({})", key_expr,);
        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some() {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| context.face.state.clone());
                }
            }
        }
        matching_subscriptions
    }
}
