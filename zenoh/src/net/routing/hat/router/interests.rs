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
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use zenoh_config::WhatAmI;
use zenoh_protocol::network::{
    declare::queryable::ext::QueryableInfoType, interest::InterestId, Interest,
};
use zenoh_runtime::ZRuntime;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        interests::{CurrentInterest, PendingCurrentInterest, RemoteInterest},
        region::Region,
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, HatBaseTrait, HatInterestTrait, HatTrait, Remote},
    router::SubscriberInfo,
};

const WARNING: &str = "Ignoring interest protocol (unsupported)"; // FIXME(regions)

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip(ctx, _msg, _src))]
    fn route_interest(
        &mut self,
        ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _src: &Remote,
    ) -> Option<CurrentInterest> {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
        None
    }

    #[tracing::instrument(level = "trace", skip(ctx, _msg))]
    fn route_interest_final(
        &mut self,
        ctx: BaseContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn route_declare_final(
        &mut self,
        ctx: BaseContext,
        _interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::error!(WARNING);
        None
    }

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn route_current_token(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
        res: Arc<Resource>,
    ) -> Option<CurrentInterest> {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());

        tracing::error!(WARNING);
        None
    }

    #[tracing::instrument(level = "trace", skip(ctx, _msg))]
    fn send_current_subscriptions(
        &self,
        ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx, _msg))]
    fn send_current_queryables(
        &self,
        ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn send_current_tokens(
        &self,
        ctx: BaseContext,
        _msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashSet<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn propagate_current_token(
        &self,
        ctx: BaseContext,
        _res: Arc<Resource>,
        _interest: CurrentInterest,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(_ctx, _dst))]
    fn send_declare_final(&mut self, _ctx: BaseContext, _id: InterestId, _dst: &Remote) {
        tracing::warn!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx, _msg), ret)]
    fn register_interest(
        &mut self,
        ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(self.region().bound().is_south());

        tracing::error!(WARNING);
    }

    #[tracing::instrument(level = "trace", skip(ctx, _msg), ret)]
    fn unregister_interest(&mut self, ctx: BaseContext, _msg: &Interest) -> Option<RemoteInterest> {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(self.region().bound().is_south());

        tracing::error!(WARNING);
        None
    }

    #[tracing::instrument(level = "trace", skip(_tables) ret)]
    fn remote_interests(&self, _tables: &TablesData) -> HashSet<RemoteInterest> {
        HashSet::default()
    }
}

impl Hat {
    #[allow(dead_code)] // FIXME(regions)
    fn register_pending_current_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        owner_hat: &mut dyn HatTrait,
        id: InterestId,
    ) {
        let Some(src) = owner_hat.new_remote(ctx.src_face, msg.ext_nodeid.node_id) else {
            return;
        };

        let cancellation_token = self.task_controller.get_cancellation_token();
        let rejection_token = self.task_controller.get_cancellation_token();

        // NOTE(regions): finalize declaration if:
        //   1. timeout expires (task)
        //   2. interest is rejected by an interceptor (by reject_interest)
        //   3. gateway finalized declaration (by task)
        {
            let cancellation_token = cancellation_token.clone();
            let rejection_token = rejection_token.clone();
            let interests_timeout = ctx.tables.interests_timeout;
            let src_face = Arc::downgrade(ctx.src_face);
            let tables_lock = ctx.tables_lock.clone();

            let cleanup = move |print_warning| {
                let _span = tracing::trace_span!(
                    "cleanup_pending_current_interest",
                    wai = %WhatAmI::Router.short(),
                    bnd = %Region::North,
                    id
                )
                .entered();

                let ctrl_lock = zlock!(tables_lock.ctrl_lock);
                let mut wtables = zwrite!(tables_lock.tables);
                let tables = &mut *wtables;

                let this = tables.hats[Region::North]
                    .as_any_mut()
                    .downcast_mut::<Hat>()
                    .unwrap();

                let Some(PendingCurrentInterest {
                    interest,
                    cancellation_token,
                    ..
                }) = this.router_pending_current_interests.remove(&id)
                else {
                    tracing::error!(id, "Unknown current interest");
                    return;
                };

                drop(ctrl_lock);

                cancellation_token.cancel();

                if print_warning {
                    tracing::warn!(
                        id = id,
                        timeout = ?interests_timeout,
                        "Gateway failed to finalize declarations for current interest before timeout",
                    );
                }

                tracing::debug!(
                    interest.src_interest_id,
                    "Finalizing declarations to source point of current interest",
                );

                let Some(mut src_face) = src_face.upgrade() else {
                    // FIXME(regions): this src face is not strictly needed,
                    // but a consequence of the BaseContext type definition.
                    // Hat impls should use Remote instead
                    tracing::error!("Could not get strong count on interest source face");
                    return;
                };

                tables.hats[interest.src_region].send_declare_final(
                    BaseContext {
                        tables_lock: &tables_lock,
                        tables: &mut tables.data,
                        src_face: &mut src_face,
                        send_declare: &mut |p, m| m.with_mut(|m| p.send_declare(m)),
                    },
                    interest.src_interest_id,
                    &interest.src,
                );
            };

            self.task_controller
                .spawn_with_rt(ZRuntime::Net, async move {
                    tokio::select! {
                        _ = cancellation_token.cancelled() => {}
                        _ = tokio::time::sleep(interests_timeout) => { cleanup(true); }
                        _ = rejection_token.cancelled() => { cleanup(false); }
                    }
                });
        }

        self.router_pending_current_interests.insert(
            id,
            PendingCurrentInterest {
                interest: Arc::new(CurrentInterest {
                    src,
                    src_interest_id: id,
                    src_region: ctx.src_face.region,
                    mode: msg.mode,
                }),
                cancellation_token,
                rejection_token,
            },
        );
    }
}
