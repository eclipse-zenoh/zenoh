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
    fmt,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zenoh_config::WhatAmI;
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::network::{
    declare::{self},
    interest::{InterestId, InterestMode, InterestOptions},
    Declare, DeclareBody, DeclareFinal, Interest,
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

use super::{face::FaceState, tables::TablesLock};
use crate::net::routing::{
    dispatcher::{face::Face, gateway::Bound, tables::Tables},
    hat::{BaseContext, HatTrait, Remote, SendDeclare},
    router::{register_expr_interest, unregister_expr_interest, NodeId, Resource},
    RoutingContext,
};

pub(crate) struct CurrentInterest {
    pub(crate) src: Remote,
    pub(crate) src_bound: Bound,
    pub(crate) src_interest_id: InterestId,
    pub(crate) mode: InterestMode,
}

pub(crate) struct PendingCurrentInterest {
    pub(crate) interest: Arc<CurrentInterest>,
    pub(crate) cancellation_token: CancellationToken,
    pub(crate) rejection_token: CancellationToken,
}

#[derive(PartialEq, Clone)]
pub(crate) struct RemoteInterest {
    pub(crate) res: Option<Arc<Resource>>,
    pub(crate) options: InterestOptions,
    pub(crate) mode: InterestMode,
}

impl fmt::Debug for RemoteInterest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteInterest")
            .field("res", &self.res.as_ref().map(|res| res.expr()))
            .field("options", &self.options)
            .finish()
    }
}

impl RemoteInterest {
    pub(crate) fn matches(&self, res: &Arc<Resource>) -> bool {
        self.res.as_ref().map(|r| r.matches(res)).unwrap_or(true)
    }
}

pub(crate) fn finalize_pending_interests(
    _tables_ref: &TablesLock,
    face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    for (_, interest) in get_mut_unchecked(face).pending_current_interests.drain() {
        finalize_pending_interest(interest, send_declare);
    }
}

pub(crate) fn finalize_pending_interest(
    pending_interest: PendingCurrentInterest,
    send_declare: &mut SendDeclare,
) {
    let interest = pending_interest.interest;
    pending_interest.cancellation_token.cancel();
    if let Some(interest) = Arc::into_inner(interest) {
        // FIXME(regions): this code is specific to peer/client hats

        let src_face = interest.src.downcast_ref::<FaceState>().unwrap();

        tracing::debug!(
            "{}:{} Propagate DeclareFinal",
            src_face,
            interest.src_interest_id
        );

        send_declare(
            &src_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(interest.src_interest_id),
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }
}

#[derive(Clone)]
pub(crate) struct CurrentInterestCleanup {
    tables: Arc<TablesLock>,
    face: Weak<FaceState>,
    id: InterestId,
    interests_timeout: Duration,
}

impl CurrentInterestCleanup {
    pub(crate) fn spawn_interest_clean_up_task(
        face: &Arc<FaceState>,
        tables_ref: &Arc<TablesLock>,
        id: u32,
        interests_timeout: Duration,
    ) {
        let mut cleanup = CurrentInterestCleanup {
            tables: tables_ref.clone(),
            face: Arc::downgrade(face),
            id,
            interests_timeout,
        };
        if let Some(pending_interest) = face.pending_current_interests.get(&id) {
            let cancellation_token = pending_interest.cancellation_token.clone();
            let rejection_token = pending_interest.rejection_token.clone();
            face.task_controller
                .spawn_with_rt(zenoh_runtime::ZRuntime::Net, async move {
                    tokio::select! {
                        _ = tokio::time::sleep(cleanup.interests_timeout) => { cleanup.run().await }
                        _ = cancellation_token.cancelled() => {}
                        _ = rejection_token.cancelled() => { cleanup.execute(false).await }
                    }
                });
        }
    }

    async fn execute(&mut self, print_warning: bool) {
        if let Some(mut face) = self.face.upgrade() {
            let ctrl_lock = zlock!(self.tables.ctrl_lock);
            if let Some(interest) = get_mut_unchecked(&mut face)
                .pending_current_interests
                .remove(&self.id)
            {
                drop(ctrl_lock);
                if print_warning {
                    tracing::warn!(
                        "{}:{} Didn't receive DeclareFinal for interest {:?}:{}: Timeout({:#?})!",
                        face,
                        self.id,
                        interest.interest.src.downcast_ref::<FaceState>().unwrap(),
                        interest.interest.src_interest_id,
                        self.interests_timeout,
                    );
                }
                finalize_pending_interest(interest, &mut |p, m| m.with_mut(|m| p.send_declare(m)));
            }
        }
    }
}

#[async_trait]
impl Timed for CurrentInterestCleanup {
    async fn run(&mut self) {
        self.execute(true).await;
    }
}

impl Face {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_interest(
        &self,
        tables_lock: &Arc<TablesLock>,
        msg: &mut Interest,
        send_declare: &mut SendDeclare,
    ) {
        let Interest {
            id,
            mode,
            options,
            wire_expr,
            ..
        } = msg;
        if options.keyexprs() && mode != &InterestMode::Current {
            register_expr_interest(
                tables_lock,
                &mut self.state.clone(),
                *id,
                wire_expr.as_ref(),
            );
        }
        let (mut res, mut wtables) = if let Some(expr) = wire_expr {
            let rtables = zread!(tables_lock.tables);
            match rtables
                .data
                .get_mapping(&self.state, &expr.scope, expr.mapping)
                .cloned()
            {
                Some(mut prefix) => {
                    tracing::debug!(
                        "{} Declare interest {} ({}{})",
                        &self.state,
                        id,
                        prefix.expr(),
                        expr.suffix
                    );
                    let res = Resource::get_resource(&prefix, &expr.suffix);
                    if res.as_ref().map(|r| r.ctx.is_some()).unwrap_or(false) {
                        drop(rtables);
                        let wtables = zwrite!(tables_lock.tables);
                        (Some(res.unwrap()), wtables)
                    } else {
                        let mut fullexpr = prefix.expr().to_string();
                        fullexpr.push_str(expr.suffix.as_ref());
                        let mut matches = keyexpr::new(fullexpr.as_str())
                            .map(|ke| Resource::get_matches(&rtables.data, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut wtables = zwrite!(tables_lock.tables);
                        let tables = &mut *wtables;
                        let mut res =
                            Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&tables.data, &mut res, matches);
                        (Some(res), wtables)
                    }
                }
                None => {
                    tracing::error!(
                        "{} Declare interest {} for unknown scope {}!",
                        &self.state,
                        id,
                        expr.scope
                    );
                    return;
                }
            }
        } else {
            (None, zwrite!(tables_lock.tables))
        };

        let tables = &mut *wtables;

        let (north_hat, south_hats) = tables.hats.partition_north_mut();

        let ctx = BaseContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare,
        };

        north_hat.route_interest(
            ctx,
            msg,
            res.as_mut(),
            south_hats.map(|d| &mut **d as &mut dyn HatTrait),
        );
    }

    pub(crate) fn undeclare_interest(&self, tables: &TablesLock, msg: &Interest) {
        tracing::debug!("{} Undeclare interest {}", &self.state, msg.id);
        unregister_expr_interest(tables, &mut self.state.clone(), msg.id);

        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let (north_hat, south_hats) = tables.hats.partition_north_mut();

        let ctx = BaseContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare: &mut |_, _| unreachable!(),
        };

        north_hat.route_interest_final(ctx, msg, south_hats.map(|d| &mut **d as &mut dyn HatTrait));
    }

    pub(crate) fn declare_final(
        &self,
        wtables: &mut Tables,
        interest_id: InterestId,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let tables = &mut *wtables;

        let ctx = BaseContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare,
        };

        // REVIEW(regions): peer initial interest
        if self.state.whatami == WhatAmI::Peer && interest_id == 0 {
            tracing::debug!(dst = %ctx.src_face, "Finalizing (peer-to-peer) initial interest");

            let peer_owner_hat = &mut tables.hats[self.state.local_bound];
            let Some(src) = peer_owner_hat.new_remote(ctx.src_face, node_id) else {
                return;
            };

            peer_owner_hat.send_final_declaration(ctx, interest_id, &src);
            return;
        }

        if !self.state.local_bound.is_north() {
            tracing::error!(
                interest_id,
                "Received current interest finalization from south/eastwest-bound face. \
                This message should only flow downstream"
            );
            return;
        }

        let (north_hat, south_hats) = tables.hats.partition_north_mut();

        north_hat.route_final_declaration(
            ctx,
            interest_id,
            south_hats.map(|d| &mut **d as &mut dyn HatTrait),
        );
    }
}
