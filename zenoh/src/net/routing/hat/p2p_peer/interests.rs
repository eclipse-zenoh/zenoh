//
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
//
use std::sync::{atomic::Ordering, Arc};

use zenoh_protocol::{
    core::WhatAmI,
    network::{
        declare::ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat, face_hat_mut, initial_interest, pubsub::declare_sub_interest,
    queries::declare_qabl_interest, token::declare_token_interest, HatCode, HatFace,
    INITIAL_INTEREST_ID,
};
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        interests::{
            CurrentInterest, CurrentInterestCleanup, PendingCurrentInterest, RemoteInterest,
        },
        resource::Resource,
        tables::{Tables, TablesLock},
    },
    hat::{CurrentFutureTrait, HatInterestTrait, SendDeclare},
    RoutingContext,
};

pub(super) fn interests_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    if face.whatami != WhatAmI::Client {
        for mut src_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Router {
                for RemoteInterest { res, options, .. } in
                    face_hat_mut!(&mut src_face).remote_interests.values()
                {
                    let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                    let face_id = face.id;
                    get_mut_unchecked(face).local_interests.insert(
                        id,
                        InterestState::new(face_id, *options, res.clone(), false),
                    );
                    let wire_expr = res.as_ref().map(|res| {
                        Resource::decl_key(res, face, super::push_declaration_profile(face))
                    });
                    face.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id,
                            mode: InterestMode::CurrentFuture,
                            options: *options,
                            wire_expr,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                        },
                        res.as_ref()
                            .map(|res| res.expr().to_string())
                            .unwrap_or_default(),
                    ));
                }
            }
        }
    }
}

impl HatInterestTrait for HatCode {
    fn declare_interest(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        options: InterestOptions,
        send_declare: &mut SendDeclare,
    ) {
        if options.subscribers() {
            declare_sub_interest(
                tables,
                face,
                id,
                res.as_deref().cloned().as_mut(),
                mode,
                options.aggregate(),
                send_declare,
            )
        }
        if options.queryables() {
            declare_qabl_interest(
                tables,
                face,
                id,
                res.as_deref().cloned().as_mut(),
                mode,
                options.aggregate(),
                send_declare,
            )
        }
        if options.tokens() {
            // Note: aggregation is forbidden for tokens. The flag is ignored.
            declare_token_interest(
                tables,
                face,
                id,
                res.as_deref().cloned().as_mut(),
                mode,
                send_declare,
            )
        }
        if mode.future() {
            face_hat_mut!(face).remote_interests.insert(
                id,
                RemoteInterest {
                    res: res.as_ref().map(|res| (*res).clone()),
                    options,
                    mode,
                },
            );
        }

        let interest = Arc::new(CurrentInterest {
            src_face: face.clone(),
            src_interest_id: id,
            mode,
        });

        if face.whatami == WhatAmI::Client {
            let propagated_mode = if mode.future() {
                InterestMode::CurrentFuture
            } else {
                mode
            };
            for dst_face in tables.faces.values_mut().filter(|f| {
                f.whatami == WhatAmI::Router
                    || (f.whatami == WhatAmI::Peer
                        && options.tokens()
                        && mode == InterestMode::Current
                        && !initial_interest(f).map(|i| i.finalized).unwrap_or(true))
            }) {
                let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                let dst_face_id = dst_face.id;
                get_mut_unchecked(dst_face).local_interests.insert(
                    id,
                    InterestState::new(
                        dst_face_id,
                        options,
                        res.as_deref().cloned(),
                        propagated_mode == InterestMode::Future,
                    ),
                );
                if mode.current() {
                    let dst_face_mut = get_mut_unchecked(dst_face);
                    let cancellation_token = dst_face_mut.task_controller.get_cancellation_token();
                    let rejection_token = dst_face_mut.task_controller.get_cancellation_token();
                    dst_face_mut.pending_current_interests.insert(
                        id,
                        PendingCurrentInterest {
                            interest: interest.clone(),
                            cancellation_token,
                            rejection_token,
                        },
                    );
                    CurrentInterestCleanup::spawn_interest_clean_up_task(
                        dst_face,
                        tables_ref,
                        id,
                        tables.interests_timeout,
                    );
                }
                let wire_expr = res.as_ref().map(|res| {
                    Resource::decl_key(res, dst_face, super::push_declaration_profile(dst_face))
                });
                dst_face.primitives.send_interest(RoutingContext::with_expr(
                    &mut Interest {
                        id,
                        mode: propagated_mode,
                        options,
                        wire_expr,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                    },
                    res.as_ref()
                        .map(|res| res.expr().to_string())
                        .unwrap_or_default(),
                ));
            }
        }

        if mode.current() {
            if let Some(interest) = Arc::into_inner(interest) {
                tracing::debug!(
                    "Propagate DeclareFinal {}:{}",
                    interest.src_face,
                    interest.src_interest_id
                );
                send_declare(
                    &interest.src_face.primitives,
                    RoutingContext::new(Declare {
                        interest_id: Some(interest.src_interest_id),
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareFinal(DeclareFinal),
                    }),
                );
            }
        }
    }

    fn undeclare_interest(&self, tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId) {
        if let Some(interest) = face_hat_mut!(face).remote_interests.remove(&id) {
            if interest.options.subscribers() {
                if interest.options.aggregate() {
                    if let Some(ires) = &interest.res {
                        face_hat_mut!(face)
                            .local_subs
                            .remove_aggregated_resource_interest(ires, id);
                    }
                } else {
                    face_hat_mut!(face)
                        .local_subs
                        .remove_simple_resource_interest(id);
                }
            }
            if interest.options.queryables() {
                if interest.options.aggregate() {
                    if let Some(ires) = &interest.res {
                        face_hat_mut!(face)
                            .local_qabls
                            .remove_aggregated_resource_interest(ires, id);
                    }
                } else {
                    face_hat_mut!(face)
                        .local_qabls
                        .remove_simple_resource_interest(id);
                }
            }
            if !tables.faces.values().any(|f| {
                f.whatami == WhatAmI::Client
                    && face_hat!(f)
                        .remote_interests
                        .values()
                        .any(|i| *i == interest)
            }) {
                for dst_face in tables
                    .faces
                    .values_mut()
                    .filter(|f| f.whatami == WhatAmI::Router)
                    .map(get_mut_unchecked)
                {
                    dst_face.local_interests.retain(|id, local_interest| {
                        if *local_interest == interest {
                            dst_face.primitives.send_interest(RoutingContext::with_expr(
                                &mut Interest {
                                    id: *id,
                                    mode: InterestMode::Final,
                                    // Note: InterestMode::Final options are undefined in the current protocol specification,
                                    //       they are initialized here for internal use by local egress interceptors.
                                    options: interest.options,
                                    wire_expr: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                },
                                local_interest
                                    .res
                                    .as_ref()
                                    .map(|res| res.expr().to_string())
                                    .unwrap_or_default(),
                            ));
                            return false;
                        }
                        true
                    });
                }
            }
        }
    }

    fn declare_final(&self, tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId) {
        if id == INITIAL_INTEREST_ID {
            zenoh_runtime::ZRuntime::Net.block_in_place(async move {
                if let Some(runtime) = &tables.runtime {
                    if let Some(runtime) = runtime.upgrade() {
                        runtime
                            .start_conditions()
                            .terminate_peer_connector_zid(face.zid)
                            .await
                    }
                }
            });
        }
    }
}
