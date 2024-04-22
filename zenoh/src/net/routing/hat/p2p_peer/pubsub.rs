// Copyright (c) 2024 ZettaScale Technology
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
use std::sync::Arc;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{Reliability, WhatAmI},
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
    if (src_face.id != dst_face.id
        || (dst_face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)))
        && !face_hat!(dst_face).local_subs.contains(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        face_hat_mut!(dst_face).local_subs.insert(res.clone());
        let key_expr = Resource::decl_key(res, dst_face);
        dst_face.primitives.send_declare(RoutingContext::with_expr(
            Declare {
                ext_qos: ext::QoSType::declare_default(),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::default(),
                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                    id: 0, // @TODO use proper SubscriberId (#703)
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
        tracing::debug!("Register subscription {} for {}", res.expr(), face);
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
                        in_interceptor_cache: None,
                        e_interceptor_cache: None,
                    }),
                );
            }
        }
    }
    face_hat_mut!(face).remote_subs.insert(res.clone());
}

fn declare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    register_client_subscription(tables, face, res, sub_info);
    let mut propa_sub_info = *sub_info;
    propa_sub_info.mode = Mode::Push;

    propagate_simple_subscription(tables, res, &propa_sub_info, face);
    // This introduced a buffer overflow on windows
    // TODO: Let's deactivate this on windows until Fixed
    #[cfg(not(windows))]
    for mcast_group in &tables.mcast_groups {
        mcast_group
            .primitives
            .send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id: 0, // @TODO use proper SubscriberId (#703)
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
                        id: 0, // @TODO use proper SubscriberId (#703)
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
    tracing::debug!("Unregister client subscription {} for {}", res.expr(), face);
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
                        id: 0, // @TODO use proper SubscriberId (#703)
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
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    undeclare_client_subscription(tables, face, res);
}

pub(super) fn pubsub_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable, // @TODO compute proper reliability to propagate from reliability of known subscribers
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
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        _node_id: NodeId,
    ) {
        declare_client_subscription(tables, face, res, sub_info);
    }

    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
    ) {
        forget_client_subscription(tables, face, res);
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<Arc<Resource>> {
        let mut subs = HashSet::new();
        for src_face in tables.faces.values() {
            for sub in &face_hat!(src_face).remote_subs {
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

    fn get_data_routes_entries(&self, _tables: &Tables) -> RoutesIndexes {
        get_routes_entries()
    }
}
