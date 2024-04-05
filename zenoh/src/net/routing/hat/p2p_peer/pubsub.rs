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
use super::{face_hat, face_hat_mut, get_routes_entries};
use super::{HatCode, HatFace};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::Tables;
use crate::net::routing::dispatcher::tables::{Route, RoutingExpr};
use crate::net::routing::hat::HatPubSubTrait;
use crate::net::routing::router::RoutesIndexes;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::network::declare::{InterestId, SubscriberId};
use zenoh_protocol::{
    core::{Reliability, WhatAmI},
    network::declare::{
        common::ext::WireExprType, ext, subscriber::ext::SubscriberInfo, Declare, DeclareBody,
        DeclareMode, DeclareSubscriber, UndeclareSubscriber,
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
    if (src_face.id != dst_face.id
        || (dst_face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)))
        && !face_hat!(dst_face).local_subs.contains_key(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_subs.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    mode: DeclareMode::Push,
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
            ));
        } else {
            let matching_interests = face_hat!(dst_face)
                .remote_sub_interests
                .values()
                .filter(|si| si.0.as_ref().map(|si| si.matches(res)).unwrap_or(true))
                .cloned()
                .collect::<Vec<(Option<Arc<Resource>>, bool)>>();

            for (int_res, aggregate) in matching_interests {
                let res = if aggregate {
                    int_res.as_ref().unwrap_or(res)
                } else {
                    res
                };
                if !face_hat!(dst_face).local_subs.contains_key(res) {
                    let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(dst_face).local_subs.insert(res.clone(), id);
                    let key_expr = Resource::decl_key(res, dst_face);
                    dst_face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            mode: DeclareMode::Push,
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
                    ));
                }
            }
        }
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
) {
    register_client_subscription(tables, face, id, res, sub_info);

    propagate_simple_subscription(tables, res, sub_info, face);
    // This introduced a buffer overflow on windows
    // TODO: Let's deactivate this on windows until Fixed
    #[cfg(not(windows))]
    for mcast_group in &tables.mcast_groups {
        mcast_group
            .primitives
            .send_declare(RoutingContext::with_expr(
                Declare {
                    mode: DeclareMode::Push,
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

#[inline]
fn remote_client_subs(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.id != face.id && ctx.subs.is_some())
}

fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(res) {
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    mode: DeclareMode::Push,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                },
                res.expr(),
            ));
        }
        for res in face_hat!(face)
            .local_subs
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade()
                    .is_some_and(|m| m.context.is_some() && remote_client_subs(&m, &face))
            }) {
                if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(&res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            mode: DeclareMode::Push,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

pub(super) fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    if !face_hat_mut!(face).remote_subs.values().any(|s| *s == *res) {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).subs = None;
        }

        let mut client_subs = client_subs(res);
        if client_subs.is_empty() {
            propagate_forget_simple_subscription(tables, res);
        }

        if client_subs.len() == 1 {
            let mut face = &mut client_subs[0];
            if !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)) {
                if let Some(id) = face_hat_mut!(face).local_subs.remove(res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            mode: DeclareMode::Push,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr(),
                    ));
                }
                for res in face_hat!(face)
                    .local_subs
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade()
                            .is_some_and(|m| m.context.is_some() && remote_client_subs(&m, face))
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(&res) {
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    mode: DeclareMode::Push,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                        id,
                                        ext_wire_expr: WireExprType::null(),
                                    }),
                                },
                                res.expr(),
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn forget_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_subs.remove(&id) {
        undeclare_client_subscription(tables, face, &mut res);
        Some(res)
    } else {
        None
    }
}

pub(super) fn pubsub_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    if face.whatami != WhatAmI::Client {
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
                );
            }
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_sub_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        continuous: bool,
        aggregate: bool,
    ) {
        if face.whatami == WhatAmI::Client {
            let mode = if continuous {
                DeclareMode::Push
            } else {
                DeclareMode::Response(id)
            };
            let sub_info = SubscriberInfo {
                reliability: Reliability::Reliable, // @TODO compute proper reliability to propagate from reliability of known subscribers
            };
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if tables.faces.values().any(|src_face| {
                        src_face.id != face.id
                            && face_hat!(src_face)
                                .remote_subs
                                .values()
                                .any(|sub| sub.context.is_some() && sub.matches(res))
                    }) {
                        let id = if continuous {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face).local_subs.insert((*res).clone(), id);
                            id
                        } else {
                            0
                        };
                        let wire_expr = Resource::decl_key(res, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                mode,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id,
                                    wire_expr,
                                    ext_info: sub_info,
                                }),
                            },
                            res.expr(),
                        ));
                    }
                } else {
                    for src_face in tables
                        .faces
                        .values()
                        .cloned()
                        .collect::<Vec<Arc<FaceState>>>()
                    {
                        if src_face.id != face.id {
                            for sub in face_hat!(src_face).remote_subs.values() {
                                if sub.context.is_some() && sub.matches(res) {
                                    let id = if continuous {
                                        let id =
                                            face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                                        face_hat_mut!(face).local_subs.insert(sub.clone(), id);
                                        id
                                    } else {
                                        0
                                    };
                                    let wire_expr = Resource::decl_key(sub, face);
                                    face.primitives.send_declare(RoutingContext::with_expr(
                                        Declare {
                                            mode,
                                            ext_qos: ext::QoSType::DECLARE,
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::DEFAULT,
                                            body: DeclareBody::DeclareSubscriber(
                                                DeclareSubscriber {
                                                    id,
                                                    wire_expr,
                                                    ext_info: sub_info,
                                                },
                                            ),
                                        },
                                        sub.expr(),
                                    ));
                                }
                            }
                        }
                    }
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    if src_face.id != face.id {
                        for sub in face_hat!(src_face).remote_subs.values() {
                            let id = if continuous {
                                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                                face_hat_mut!(face).local_subs.insert(sub.clone(), id);
                                id
                            } else {
                                0
                            };
                            let wire_expr = Resource::decl_key(sub, face);
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    mode,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id,
                                        wire_expr,
                                        ext_info: sub_info,
                                    }),
                                },
                                sub.expr(),
                            ));
                        }
                    }
                }
            }
        }
        if continuous {
            face_hat_mut!(face)
                .remote_sub_interests
                .insert(id, (res.cloned(), aggregate));
        }
    }

    fn undeclare_sub_interest(
        &self,
        _tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
    ) {
        face_hat_mut!(face).remote_sub_interests.remove(&id);
    }

    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        _node_id: NodeId,
    ) {
        declare_client_subscription(tables, face, id, res, sub_info);
    }

    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        forget_client_subscription(tables, face, id)
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<Arc<Resource>> {
        let mut subs = HashSet::new();
        for src_face in tables.faces.values() {
            for sub in face_hat!(src_face).remote_subs.values() {
                subs.insert(sub.clone());
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
                if context.subs.is_some()
                    && match tables.whatami {
                        WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                        _ => {
                            source_type == WhatAmI::Client
                                || context.face.whatami == WhatAmI::Client
                        }
                    }
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
}
