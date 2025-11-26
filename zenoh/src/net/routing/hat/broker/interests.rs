//
// Copyright (c) 2025 ZettaScale Technology
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
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};

use itertools::Itertools;
use zenoh_protocol::network::{
    declare, interest::InterestId, Declare, DeclareBody, DeclareFinal, DeclareSubscriber, Interest,
};

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        interests::{CurrentInterest, RemoteInterest},
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, Remote},
    router::SubscriberInfo,
    RoutingContext,
};
impl Hat {
    pub(super) fn interests_new_face(&self, _ctx: BaseContext) {
        // The broker hat is never the north hat, thus there are no interests to re-propagate
    }
}

impl HatInterestTrait for Hat {
    fn route_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _remote: &Remote,
    ) -> Option<CurrentInterest> {
        // The broker hat is never the north hat, thus it doesn't route interests
        unreachable!()
    }

    fn route_interest_final(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        // The broker hat is never the north hat, thus it doesn't route interests
        unreachable!()
    }

    fn route_declare_final(
        &mut self,
        _ctx: BaseContext,
        _interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        // The broker hat is never the north hat, thus it doesn't route current declarations
        unreachable!()
    }

    fn send_declarations(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<&mut Arc<Resource>>,
    ) {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_current_subscriptions(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        mut matches: HashSet<Arc<Resource>>,
    ) {
        assert!(self.owns(ctx.src_face));
        assert!(ctx.src_face.region.bound().is_south());

        matches.extend(
            self.owned_faces(ctx.tables)
                .filter(|face| face.id != ctx.src_face.id)
                .flat_map(|face| self.face_hat(face).remote_subs.values().cloned()),
        );
        let mut matches = matches.into_iter();
        let push = false;

        if msg.options.aggregate() && (msg.mode.current() || msg.mode.future()) {
            if let Some(aggregated_res) = &res {
                let (sub_id, sub_info) = if msg.mode.future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);

                    for sub in matches {
                        face_hat_mut.local_subs.insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }

                    face_hat_mut.local_subs.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([msg.id]),
                    )
                } else {
                    (0, matches.next().map(|_| SubscriberInfo))
                };

                if msg.mode.current() && sub_info.is_some() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face, push);
                    (ctx.send_declare)(
                        &ctx.src_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(msg.id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: sub_id,
                                    wire_expr,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !msg.options.aggregate() && msg.mode.current() {
            for sub in matches {
                let sub_id = if msg.mode.future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut
                        .local_subs
                        .insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([msg.id]),
                        )
                        .0
                } else {
                    0
                };
                let wire_expr = Resource::decl_key(&sub, ctx.src_face, push);
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(msg.id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: sub_id,
                                wire_expr,
                            }),
                        },
                        sub.expr().to_string(),
                    ),
                );
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn send_declare_final(&mut self, ctx: BaseContext, id: InterestId, src: &Remote) {
        // I should send a DeclareFinal to the source of the current interest identified by the given IID and FID

        let src_face = self.hat_remote(src);

        (ctx.send_declare)(
            &src_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn register_interest(&mut self, ctx: BaseContext, msg: &Interest, res: Option<Arc<Resource>>) {
        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res,
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        assert!(self.region().bound().is_south());
        assert!(self.owns(&ctx.src_face));

        let Some(remote_interest) = self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .remove(&msg.id)
        else {
            tracing::error!(id = msg.id, "Unknown remote interest");
            return None;
        };

        self.owned_faces(ctx.tables)
            .all(|face| {
                !self
                    .face_hat(face)
                    .remote_interests
                    .values()
                    .contains(&remote_interest)
            })
            .then_some(remote_interest)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest> {
        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_interests.values().cloned())
            .collect()
    }
}
