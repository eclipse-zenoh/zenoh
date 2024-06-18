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

use super::{face_hat, face_hat_mut, token::declare_token_interest, HatCode, HatFace};
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        interests::{CurrentInterest, CurrentInterestCleanup},
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
            for (res, options) in face_hat_mut!(&mut src_face).remote_interests.values() {
                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                get_mut_unchecked(face).local_interests.insert(
                    id,
                    InterestState {
                        options: *options,
                        res: res.as_ref().map(|res| (*res).clone()),
                        finalized: false,
                    },
                );
                let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, face));
                face.primitives.send_interest(RoutingContext::with_expr(
                    Interest {
                        id,
                        mode: InterestMode::CurrentFuture,
                        options: *options,
                        wire_expr,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                    },
                    res.as_ref().map(|res| res.expr()).unwrap_or_default(),
                ));
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
        if options.tokens() {
            declare_token_interest(
                tables,
                face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
                send_declare,
            )
        }
        face_hat_mut!(face)
            .remote_interests
            .insert(id, (res.as_ref().map(|res| (*res).clone()), options));

        let interest = Arc::new(CurrentInterest {
            src_face: face.clone(),
            src_interest_id: id,
        });

        for dst_face in tables
            .faces
            .values_mut()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            get_mut_unchecked(dst_face).local_interests.insert(
                id,
                InterestState {
                    options,
                    res: res.as_ref().map(|res| (*res).clone()),
                    finalized: mode == InterestMode::Future,
                },
            );
            if mode.current() && options.tokens() {
                let dst_face_mut = get_mut_unchecked(dst_face);
                let cancellation_token = dst_face_mut.task_controller.get_cancellation_token();
                dst_face_mut
                    .pending_current_interests
                    .insert(id, (interest.clone(), cancellation_token));
                CurrentInterestCleanup::spawn_interest_clean_up_task(dst_face, tables_ref, id);
            }
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, dst_face));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                Interest {
                    id,
                    mode,
                    options,
                    wire_expr,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                },
                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
            ));
        }

        if mode.current() {
            if options.tokens() {
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
            } else {
                send_declare(
                    &face.primitives,
                    RoutingContext::new(Declare {
                        interest_id: Some(id),
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
                    .filter(|f| f.whatami != WhatAmI::Client)
                {
                    for id in dst_face
                        .local_interests
                        .keys()
                        .cloned()
                        .collect::<Vec<InterestId>>()
                    {
                        let local_interest = dst_face.local_interests.get(&id).unwrap();
                        if local_interest.res == interest.0 && local_interest.options == interest.1
                        {
                            dst_face.primitives.send_interest(RoutingContext::with_expr(
                                Interest {
                                    id,
                                    mode: InterestMode::Final,
                                    options: InterestOptions::empty(),
                                    wire_expr: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                },
                                local_interest
                                    .res
                                    .as_ref()
                                    .map(|res| res.expr())
                                    .unwrap_or_default(),
                            ));
                            get_mut_unchecked(dst_face).local_interests.remove(&id);
                        }
                    }
                }
            }
        }
    }
}
