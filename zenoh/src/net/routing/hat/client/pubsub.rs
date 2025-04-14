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
    core::{key_expr::OwnedKeyExpr, WhatAmI},
    network::declare::{
        common::ext::WireExprType, ext, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
        UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::{Face, FaceState},
            pubsub::SubscriberInfo,
            resource::{NodeId, Resource, SessionContext},
            tables::{Route, RoutingExpr, Tables},
        },
        hat::{HatPubSubTrait, SendDeclare, Sources},
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
    if src_face.state.id != dst_face.state.id
        && !face_hat!(dst_face.state).local_subs.contains_key(res)
        && (src_face.state.whatami == WhatAmI::Client || dst_face.state.whatami == WhatAmI::Client)
    {
        let id = face_hat!(dst_face.state)
            .next_id
            .fetch_add(1, Ordering::SeqCst);
        face_hat_mut!(&mut dst_face.state)
            .local_subs
            .insert(res.clone(), id);
        let key_expr = Resource::decl_key(res, dst_face, true);
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

fn propagate_forget_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
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
    let sub_info = SubscriberInfo;
    for src_face in tables.faces.values().cloned().collect::<Vec<_>>() {
        for sub in face_hat!(src_face.state).remote_subs.values() {
            propagate_simple_subscription_to(tables, face, sub, &sub_info, &src_face, send_declare);
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
                .filter(|f| f.state.whatami != WhatAmI::Client)
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
                        (face.state.clone(), key_expr.to_owned(), NodeId::default()),
                    );
                }
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
                if context.subs.is_some() && context.face.state.whatami == WhatAmI::Client {
                    route.entry(*sid).or_insert_with(|| {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                        (
                            context.face.state.clone(),
                            key_expr.to_owned(),
                            NodeId::default(),
                        )
                    });
                }
            }
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

        for face in tables
            .faces
            .values()
            .filter(|f| f.state.whatami != WhatAmI::Client)
        {
            if face.state.local_interests.values().any(|interest| {
                interest.finalized
                    && interest.options.subscribers()
                    && interest
                        .res
                        .as_ref()
                        .map(|res| KeyExpr::keyexpr_include(res.expr(), key_expr))
                        .unwrap_or(true)
            }) && face_hat!(face.state)
                .remote_subs
                .values()
                .any(|sub| KeyExpr::keyexpr_intersect(sub.expr(), key_expr))
            {
                matching_subscriptions.insert(face.state.id, face.state.clone());
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
                if context.subs.is_some() && context.face.state.whatami == WhatAmI::Client {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| context.face.state.clone());
                }
            }
        }
        matching_subscriptions
    }
}
