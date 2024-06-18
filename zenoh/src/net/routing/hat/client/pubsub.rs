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
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, Reliability, WhatAmI},
    network::declare::{
        common::ext::WireExprType, ext, subscriber::ext::SubscriberInfo, Declare, DeclareBody,
        DeclareSubscriber, SubscriberId, UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, get_routes_entries, HatCode, HatFace};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            resource::{NodeId, Resource, SessionContext},
            tables::{Route, RoutingExpr, Tables},
        },
        hat::{HatPubSubTrait, SendDeclare, Sources},
        router::{update_data_routes_from, RoutesIndexes},
        RoutingContext,
    },
};

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if src_face.id != dst_face.id
        && !face_hat!(dst_face).local_subs.contains_key(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
        face_hat_mut!(dst_face).local_subs.insert(res.clone(), id);
        let key_expr = Resource::decl_key(res, dst_face);
        send_declare(
            &dst_face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id,
                        wire_expr: key_expr,
                        ext_info: *sub_info,
                    }),
                },
                res.expr(),
            ),
        );
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

fn register_client_subscription(
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

fn declare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    send_declare: &mut SendDeclare,
) {
    register_client_subscription(tables, face, id, res, sub_info);

    propagate_simple_subscription(tables, res, sub_info, face, send_declare);
    // This introduced a buffer overflow on windows
    // @TODO: Let's deactivate this on windows until Fixed
    #[cfg(not(windows))]
    for mcast_group in &tables.mcast_groups {
        mcast_group
            .primitives
            .send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id: 0, // @TODO use proper SubscriberId
                        wire_expr: res.expr().into(),
                        ext_info: *sub_info,
                    }),
                },
                res.expr(),
            ))
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

fn propagate_forget_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
        if let Some(id) = face_hat_mut!(face).local_subs.remove(res) {
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
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
                    res.expr(),
                ),
            );
        }
    }
}

pub(super) fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    if !face_hat_mut!(face).remote_subs.values().any(|s| *s == *res) {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).subs = None;
        }

        let mut client_subs = client_subs(res);
        if client_subs.is_empty() {
            propagate_forget_simple_subscription(tables, res, send_declare);
        }
        if client_subs.len() == 1 {
            let face = &mut client_subs[0];
            if let Some(id) = face_hat_mut!(face).local_subs.remove(res) {
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
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
                        res.expr(),
                    ),
                );
            }
        }
    }
}

fn forget_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_subs.remove(&id) {
        undeclare_client_subscription(tables, face, &mut res, send_declare);
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
    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable, // @TODO compute proper reliability to propagate from reliability of known subscribers
    };
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
    // recompute routes
    update_data_routes_from(tables, &mut tables.root_res.clone());
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
        declare_client_subscription(tables, face, id, res, sub_info, send_declare);
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
        forget_client_subscription(tables, face, id, send_declare)
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known suscriptions (keys)
        let mut subs = HashMap::new();
        for src_face in tables.faces.values() {
            for sub in face_hat!(src_face).remote_subs.values() {
                // Insert the key in the list of known suscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a suscription source in the proper list
                match src_face.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.zid),
                }
            }
        }
        Vec::from_iter(subs)
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

        for face in tables
            .faces
            .values()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            if face.local_interests.values().any(|interest| {
                interest.finalized
                    && interest.options.subscribers()
                    && interest
                        .res
                        .as_ref()
                        .map(|res| {
                            KeyExpr::try_from(res.expr())
                                .and_then(|intres| {
                                    KeyExpr::try_from(expr.full_expr())
                                        .map(|putres| intres.includes(&putres))
                                })
                                .unwrap_or(false)
                        })
                        .unwrap_or(true)
            }) {
                if face_hat!(face).remote_subs.values().any(|sub| {
                    KeyExpr::try_from(sub.expr())
                        .and_then(|subres| {
                            KeyExpr::try_from(expr.full_expr())
                                .map(|putres| subres.intersects(&putres))
                        })
                        .unwrap_or(false)
                }) {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                    route.insert(
                        face.id,
                        (face.clone(), key_expr.to_owned(), NodeId::default()),
                    );
                }
            } else {
                let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                route.insert(
                    face.id,
                    (face.clone(), key_expr.to_owned(), NodeId::default()),
                );
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
                if context.subs.is_some() && context.face.whatami == WhatAmI::Client {
                    route.entry(*sid).or_insert_with(|| {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                        (context.face.clone(), key_expr.to_owned(), NodeId::default())
                    });
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

    fn get_data_routes_entries(&self, _tables: &Tables) -> RoutesIndexes {
        get_routes_entries()
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

        for face in tables
            .faces
            .values()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            if face.local_interests.values().any(|interest| {
                interest.finalized
                    && interest.options.subscribers()
                    && interest
                        .res
                        .as_ref()
                        .map(|res| {
                            KeyExpr::try_from(res.expr())
                                .map(|intres| intres.includes(key_expr))
                                .unwrap_or(false)
                        })
                        .unwrap_or(true)
            }) && face_hat!(face).remote_subs.values().any(|sub| {
                KeyExpr::try_from(sub.expr())
                    .map(|subres| subres.intersects(key_expr))
                    .unwrap_or(false)
            }) {
                matching_subscriptions.insert(face.id, face.clone());
            }
        }

        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some() && context.face.whatami == WhatAmI::Client {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| context.face.clone());
                }
            }
        }
        matching_subscriptions
    }
}
