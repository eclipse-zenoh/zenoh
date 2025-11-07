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

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _remote: &Remote,
    ) -> Option<CurrentInterest> {
        // I have received an interest with mode != FINAL.
        // I should be the north hat.
        // Either I own the src face, in which case msg originates in my region and I need to pass the parcel till the gateway.
        // Or, the face is south-bound, in which case the msg originates in a subregion (for which I am the gateway):
        //   1. If I have a gateway, I should re-propagate the interest to it.
        //   2. If the interest is current, I need to send all current declarations in the (south) owner hat.
        //   3. If the interest is future, I need to register it as a remote interest in the (south) owner hat.

        debug_assert!(self.region().bound().is_north());

        tracing::warn!("Router regions do not support routing interests");
        None
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_interest_final(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        // I have received an interest with mode FINAL.
        // I should be the north hat.
        // Either I own the src face, in which case msg originates in my region and I need to pass the parcel till the gateway.
        // Or, the face is south-bound, in which case the msg originates in a subregion (for I which am the gateway):
        //   1. I need to unregister it as a remote interest in the owner (south) hat.
        //   2. If I have a gateway, I should re-propagate the FINAL interest to it iff no other subregion has the same remote interest.

        debug_assert!(self.region().bound().is_north());

        tracing::warn!("Router regions do not support routing interests");
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_declare_final(
        &mut self,
        _ctx: BaseContext,
        _interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        unreachable!() // REVIEW(regions)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
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
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_current_queryables(
        &self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn send_declare_final(&mut self, _ctx: BaseContext, _id: InterestId, _src: &Remote) {
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn register_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
    ) {
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn unregister_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
    ) -> Option<RemoteInterest> {
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn remote_interests(&self, _tables: &TablesData) -> HashSet<RemoteInterest> {
        HashSet::default()
    }
}

impl Hat {
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
