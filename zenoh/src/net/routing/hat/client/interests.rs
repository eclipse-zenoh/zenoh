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
    core::{WhatAmI, ZenohIdProto},
    network::{
        ext,
        interest::{self, InterestId, InterestMode},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        gateway::BoundMap,
        interests::{
            finalize_pending_interest, CurrentInterest, CurrentInterestCleanup,
            PendingCurrentInterest, RemoteInterest,
        },
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, CurrentFutureTrait, HatInterestTrait, HatTrait},
    RoutingContext,
};
impl Hat {
    pub(super) fn interests_new_face(&self, tables: &mut TablesData, face: &mut Arc<FaceState>) {
        if face.whatami != WhatAmI::Client {
            for mut src_face in self
                .faces(tables)
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for RemoteInterest { res, options, .. } in
                    self.face_hat_mut(&mut src_face).remote_interests.values()
                {
                    let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                    get_mut_unchecked(face).local_interests.insert(
                        id,
                        InterestState {
                            options: *options,
                            res: res.as_ref().map(|res| (*res).clone()),
                            finalized: false,
                        },
                    );
                    let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, face, true));
                    face.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id,
                            mode: InterestMode::CurrentFuture,
                            options: *options,
                            wire_expr,
                            ext_qos: interest::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: interest::ext::NodeIdType::DEFAULT,
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

impl HatInterestTrait for Hat {
    fn propagate_declarations(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        upstream_hat: &mut dyn HatTrait,
    ) {
        if msg.options.tokens() {
            self.declare_token_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }

        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res: res.as_ref().map(|res| (*res).clone()),
                options: msg.options,
                mode: msg.mode,
            },
        );

        let src_zid = ctx.src_face.zid;

        if !upstream_hat.propagate_interest(ctx.reborrow(), msg, res, &src_zid)
            && msg.mode.current()
            && msg.options.tokens()
        {
            self.finalize_current_interest(ctx, msg.id, &src_zid);
        }
    }

    fn propagate_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        zid: &ZenohIdProto,
    ) -> bool {
        let interest = Arc::new(CurrentInterest {
            src_face: ctx.src_face.clone(),
            src_interest_id: msg.id,
            mode: msg.mode,
        });

        let interests_timeout = ctx.tables.interests_timeout;

        for mut dst_face in self
            .faces(ctx.tables)
            .values()
            .filter(|f| f.whatami != WhatAmI::Client)
            .cloned()
            .collect::<Vec<_>>()
        {
            let id = self
                .face_hat(&dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            get_mut_unchecked(&mut dst_face).local_interests.insert(
                id,
                InterestState {
                    options: msg.options,
                    res: res.as_ref().map(|res| (*res).clone()),
                    finalized: msg.mode == InterestMode::Future,
                },
            );
            if msg.mode.current() && msg.options.tokens() {
                let dst_face_mut = get_mut_unchecked(&mut dst_face);
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
                    &dst_face,
                    ctx.tables_lock,
                    id,
                    interests_timeout,
                );
            }
            let wire_expr = res
                .as_ref()
                .map(|res| Resource::decl_key(res, &mut dst_face, true));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                &mut Interest {
                    id,
                    mode: msg.mode,
                    options: msg.options,
                    wire_expr,
                    ext_qos: interest::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                },
                res.as_ref()
                    .map(|res| res.expr().to_string())
                    .unwrap_or_default(),
            ));
        }

        Arc::strong_count(&interest) > 1
    }

    fn unregister_current_interest(
        &mut self,
        mut ctx: BaseContext,
        id: InterestId,
        mut downstream_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        if self.owns(ctx.src_face) {
            if let Some(interest) = get_mut_unchecked(&mut ctx.src_face.clone())
                .pending_current_interests
                .remove(&id)
            {
                finalize_pending_interest(interest, ctx.send_declare);
            }
        } else {
            // REVIEW(regions): not sure
            for mut face in self
                .faces(ctx.tables)
                .values()
                .filter(|face| face.whatami != WhatAmI::Client)
                .cloned()
                .collect::<Vec<_>>()
            {
                if let Some(pending_interest) = get_mut_unchecked(&mut face)
                    .pending_current_interests
                    .remove(&id)
                {
                    pending_interest.cancellation_token.cancel();

                    let hat = &mut downstream_hats[pending_interest.interest.src_face.bound];
                    hat.finalize_current_interest(
                        ctx.reborrow(),
                        id,
                        &pending_interest.interest.src_face.zid,
                    );
                }
            }
        }
    }

    fn finalize_current_interest(&mut self, ctx: BaseContext, id: InterestId, zid: &ZenohIdProto) {
        if let Some(face) = self.face(ctx.tables, zid) {
            (ctx.send_declare)(
                &face.primitives,
                RoutingContext::new(Declare {
                    interest_id: Some(id),
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                }),
            );
        } else {
            todo!()
        }
    }

    fn unregister_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        upstream_hat: &mut dyn HatTrait,
    ) {
        if let Some(inbound_interest) = self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .remove(&msg.id)
        {
            if !self.faces(ctx.tables).values().any(|f| {
                f.whatami == WhatAmI::Client
                    && self
                        .face_hat(f)
                        .remote_interests
                        .values()
                        .any(|i| i == &inbound_interest)
            }) {
                upstream_hat.finalize_interest(ctx, msg, inbound_interest);
            }
        }
    }

    fn finalize_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        inbound_interest: RemoteInterest,
    ) {
        for dst_face in self
            .faces_mut(ctx.tables)
            .values_mut()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            for id in dst_face.local_interests.keys().cloned().collect::<Vec<_>>() {
                let local_interest = dst_face.local_interests.get(&id).unwrap();
                if local_interest.res == inbound_interest.res
                    && local_interest.options == inbound_interest.options
                {
                    dst_face.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id,
                            mode: InterestMode::Final,
                            // NOTE: InterestMode::Final options are undefined in the current protocol specification,
                            // they are initialized here for internal use by local egress interceptors.
                            options: inbound_interest.options,
                            wire_expr: None,
                            ext_qos: interest::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                        },
                        local_interest
                            .res
                            .as_ref()
                            .map(|res| res.expr().to_string())
                            .unwrap_or_default(),
                    ));
                    get_mut_unchecked(dst_face).local_interests.remove(&id);
                }
            }
        }
    }
}
