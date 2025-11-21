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
    core::WhatAmI,
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
            face::FaceState,
            pubsub::SubscriberInfo,
            resource::{NodeId, Resource, SessionContext},
            tables::{Route, RoutingExpr, Tables},
        },
        hat::{
            p2p_peer::{initial_interest, INITIAL_INTEREST_ID},
            CurrentFutureTrait, HatPubSubTrait, SendDeclare, Sources,
        },
        router::RouteBuilder,
        RoutingContext,
    },
};

#[inline]
fn maybe_register_local_subscriber(
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    initial_interest: Option<InterestId>,
    send_declare: &mut SendDeclare,
) {
    if face_hat!(dst_face).local_subs.contains_simple_resource(res) {
        return;
    }
    let (should_notify, simple_interests) = match initial_interest {
        Some(interest) => (true, HashSet::from([interest])),
        None => face_hat!(dst_face)
            .remote_interests
            .iter()
            .filter(|(_, i)| i.options.subscribers() && i.matches(res))
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
    let face_hat_mut = face_hat_mut!(dst_face);
    let (_, subs_to_notify) = face_hat_mut.local_subs.insert_simple_resource(
        res.clone(),
        SubscriberInfo,
        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
        simple_interests,
    );

    for update in subs_to_notify {
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
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id: update.id,
                        wire_expr: key_expr.clone(),
                    }),
                },
                update.resource.expr().to_string(),
            ),
        );
    }
}

#[inline]
fn maybe_unregister_local_subscriber(
    face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for update in face_hat_mut!(face).local_subs.remove_simple_resource(res) {
        send_declare(
            &face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: update.id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                },
                update.resource.expr().to_string(),
            ),
        );
    }
}

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    _sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if (src_face.id != dst_face.id)
        && !face_hat!(dst_face).local_subs.contains_simple_resource(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        if dst_face.whatami != WhatAmI::Client {
            maybe_register_local_subscriber(dst_face, res, Some(INITIAL_INTEREST_ID), send_declare);
        } else {
            maybe_register_local_subscriber(dst_face, res, None, send_declare);
        }
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
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
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => {
                if ctx.subs.is_none() {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).subs = Some(*sub_info);
            }
        }
    }
    face_hat_mut!(face).remote_subs.insert(id, res.clone());
}

fn declare_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
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
    if face.whatami == WhatAmI::Client {
        for mcast_group in &tables.mcast_groups {
            if mcast_group.mcast_group != face.mcast_group {
                mcast_group
                    .primitives
                    .send_declare(RoutingContext::with_expr(
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
                        res.expr().to_string(),
                    ))
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
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

fn propagate_forget_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for mut face in tables.faces.values().cloned() {
        maybe_unregister_local_subscriber(&mut face, res, send_declare);
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
            let face = &mut simple_subs[0];
            maybe_unregister_local_subscriber(face, res, send_declare);
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
    face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if face.whatami != WhatAmI::Client {
        let sub_info = SubscriberInfo;
        for src_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for sub in face_hat!(src_face).remote_subs.values() {
                propagate_simple_subscription_to(
                    tables,
                    face,
                    sub,
                    &sub_info,
                    &mut src_face.clone(),
                    send_declare,
                );
            }
        }
    }
}

fn get_subscribers_matching_resource<'a>(
    tables: &'a Tables,
    face: &Arc<FaceState>,
    res: Option<&'a Arc<Resource>>,
) -> impl Iterator<Item = &'a Arc<Resource>> {
    let face_id = face.id;

    tables
        .faces
        .values()
        .filter(move |f| f.id != face_id)
        .flat_map(|f| face_hat!(f).remote_subs.values())
        .filter(move |sub| sub.context.is_some() && res.map(|r| sub.matches(r)).unwrap_or(true))
}

pub(super) fn declare_sub_interest(
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
    let mut matching_subs = get_subscribers_matching_resource(tables, face, res.as_ref());

    if aggregate && (mode.current() || mode.future()) {
        if let Some(aggregated_res) = &res {
            let (resource_id, sub_info) = if mode.future() {
                let face_hat_mut = face_hat_mut!(face);
                for sub in matching_subs {
                    face_hat_mut.local_subs.insert_simple_resource(
                        sub.clone(),
                        SubscriberInfo,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::new(),
                    );
                }
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut.local_subs.insert_aggregated_resource(
                    aggregated_res.clone(),
                    || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                    HashSet::from_iter([interest_id]),
                )
            } else {
                (0, matching_subs.next().map(|_| SubscriberInfo))
            };
            if mode.current() && sub_info.is_some() {
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
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: resource_id,
                                wire_expr,
                            }),
                        },
                        aggregated_res.expr().to_string(),
                    ),
                );
            }
        }
    } else if !aggregate && mode.current() {
        for sub in matching_subs {
            let resource_id = if mode.future() {
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut
                    .local_subs
                    .insert_simple_resource(
                        sub.clone(),
                        SubscriberInfo,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from([interest_id]),
                    )
                    .0
            } else {
                0
            };
            let wire_expr = Resource::decl_key(sub, face, super::push_declaration_profile(face));
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(interest_id),
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id: resource_id,
                            wire_expr,
                        }),
                    },
                    sub.expr().to_string(),
                ),
            );
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
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
        // Compute the list of known subscriptions (keys)
        let mut subs = HashMap::new();
        for face in tables.faces.values() {
            for sub in face_hat!(face).remote_subs.values() {
                // Insert the key in the list of known subscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a subscription source in the proper list
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
        Vec::from_iter(subs)
    }

    fn get_publications(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face).remote_interests.values() {
                if interest.options.subscribers() {
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

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route> {
        let mut route = RouteBuilder::new();
        let Some(key_expr) = expr.key_expr() else {
            return Arc::new(route.build());
        };
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
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

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some()
                    && (source_type == WhatAmI::Client || context.face.whatami == WhatAmI::Client)
                {
                    route.insert(*sid, || {
                        let wire_expr = expr.get_best_key(*sid);
                        (
                            context.face.clone(),
                            wire_expr.to_owned(),
                            NodeId::default(),
                        )
                    });
                }
            }
        }

        if source_type == WhatAmI::Client {
            for face in tables.faces.values() {
                if face.whatami == WhatAmI::Router {
                    route.try_insert(face.id, || {
                        let has_interest_finalized = expr
                            .resource()
                            .and_then(|res| res.session_ctxs.get(&face.id))
                            .is_some_and(|ctx| ctx.subscriber_interest_finalized);
                        (!has_interest_finalized).then(|| {
                            let wire_expr = expr.get_best_key(face.id);
                            (face.clone(), wire_expr.to_owned(), NodeId::default())
                        })
                    });
                } else if face.whatami == WhatAmI::Peer
                    && initial_interest(face).is_some_and(|i| !i.finalized)
                {
                    route.insert(face.id, || {
                        let wire_expr = expr.get_best_key(face.id);
                        (face.clone(), wire_expr.to_owned(), NodeId::default())
                    });
                }
            }
        }

        for mcast_group in &tables.mcast_groups {
            route.insert(mcast_group.id, || {
                (
                    mcast_group.clone(),
                    key_expr.to_string().into(),
                    NodeId::default(),
                )
            });
        }
        Arc::new(route.build())
    }

    fn get_matching_subscriptions(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_subscriptions = HashMap::new();

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
                        .or_insert_with(|| context.face.clone());
                }
            }
        }
        matching_subscriptions
    }
}
