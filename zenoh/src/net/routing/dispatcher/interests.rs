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
    fmt::{self, Debug},
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zenoh_protocol::{
    core::Region,
    network::{
        declare::{self},
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

use super::{face::FaceState, tables::TablesLock};
use crate::net::routing::{
    dispatcher::{face::Face, tables::Tables},
    gateway::{register_expr_interest, NodeId, Resource},
    hat::{DispatcherContext, Remote, RouteCurrentDeclareResult, RouteInterestResult, SendDeclare},
    RoutingContext,
};

#[derive(Debug, Clone)]
pub(crate) struct CurrentInterest {
    pub(crate) src: Remote,
    pub(crate) src_region: Region,
    pub(crate) src_interest_id: InterestId,
    pub(crate) mode: InterestMode,
}

pub(crate) struct PendingCurrentInterest {
    pub(crate) interest: Arc<CurrentInterest>,
    pub(crate) cancellation_token: CancellationToken,
    pub(crate) rejection_token: CancellationToken,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct RemoteInterest {
    pub(crate) res: Option<Arc<Resource>>,
    pub(crate) options: InterestOptions,
    pub(crate) mode: InterestMode,
}

impl fmt::Debug for RemoteInterest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteInterest")
            .field("res", &self.res.as_ref().map(|res| res.expr()))
            .field("opts", &self.options)
            .field("mode", &self.mode)
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
        // FIXME(regions): this is only safe as long as router interests remain unimplemented
        let src_face = interest
            .src
            .downcast_ref_to_face()
            .expect("interest source remote should be a face");

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
                        interest.interest.src.downcast_ref_to_face(),
                        interest.interest.src_interest_id,
                        self.interests_timeout,
                    );
                }
                finalize_pending_interest(interest, &mut |p, m| {
                    m.with_mut(|m| {
                        p.send_declare(m);
                    })
                });
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
    #[tracing::instrument(
        level = "debug", 
        skip(self, msg, send_declare),
        fields(
            id = msg.id,
            mode = ?msg.mode,
            opts = %msg.options,
            expr = msg.wire_expr.as_ref().map(|we| we.to_string())
        ),
        ret
    )]
    pub(crate) fn interest(&self, msg: &mut Interest, send_declare: &mut SendDeclare) {
        let region = self.state.region;

        if region.bound().is_north() && !self.state.whatami.is_peer() {
            tracing::error!(
                src = %self.state,
                "Ignoring interest from non-peer north-bound face (illegal)"
            );
            return;
        }

        if self.state.whatami.is_router() {
            tracing::warn!("Ignoring interest from router (unsupported)");
            return;
        }

        if msg.options.aggregate() && self.state.whatami.is_peer() {
            tracing::warn!("Ignoring aggregate interest option from peer (unsupported)");
            msg.options -= InterestOptions::AGGREGATE;
        }

        if msg.options.aggregate() && msg.options.tokens() {
            tracing::error!("Ignoring aggregate interest option for tokens (illegal)");
            msg.options -= InterestOptions::AGGREGATE;
        }

        if msg.mode == InterestMode::Current
            && (msg.options.subscribers() || msg.options.queryables() || !msg.options.tokens())
        {
            tracing::error!("Current interests may only refer to tokens (illegal)");
            return;
        }

        let msg = &*msg;

        let Interest {
            id,
            mode,
            options,
            wire_expr,
            ..
        } = msg;

        if options.keyexprs() && mode != &InterestMode::Current {
            register_expr_interest(
                &self.tables,
                &mut self.state.clone(),
                *id,
                wire_expr.as_ref(),
            );
        }

        self.with_mapped_optional_expr(wire_expr.as_ref(), |tables, res| {
            let hats = &mut tables.hats;

            let mut ctx = DispatcherContext {
                tables_lock: &self.tables,
                tables: &mut tables.data,
                src_face: &mut self.state.clone(),
                send_declare,
            };

            let Some(src) = hats[region].new_remote(ctx.src_face, msg.ext_nodeid.node_id) else {
                return;
            };

            let route_interest_res =
                hats[Region::North].route_interest(ctx.reborrow(), msg, res.clone(), &src);

            if msg.mode.is_current() {
                if msg.options.subscribers() {
                    let other_sub_matches = hats
                        .values()
                        .filter(|hat| hat.region() != region)
                        .flat_map(|hat| {
                            hat.remote_subscribers_matching(ctx.tables, res.as_deref())
                                .into_iter()
                        })
                        .collect::<HashMap<_, _>>();

                    hats[region].send_current_subscribers(
                        ctx.reborrow(),
                        msg,
                        res.clone(),
                        other_sub_matches,
                    );
                }

                if msg.options.queryables() {
                    let other_qabl_matches = hats
                        .values()
                        .filter(|hat| hat.region() != region)
                        .flat_map(|hat| {
                            hat.remote_queryables_matching(ctx.tables, res.as_deref())
                                .into_iter()
                        })
                        .collect::<HashMap<_, _>>();
                    hats[region].send_current_queryables(
                        ctx.reborrow(),
                        msg,
                        res.clone(),
                        other_qabl_matches,
                    );
                }

                if msg.options.tokens() {
                    let other_token_matches = hats
                        .values()
                        .filter(|hat| hat.region() != region)
                        .flat_map(|hat| {
                            hat.remote_tokens_matching(ctx.tables, res.as_deref())
                                .into_iter()
                        })
                        .collect::<HashSet<_>>();
                    hats[region].send_current_tokens(
                        ctx.reborrow(),
                        msg,
                        res.clone(),
                        other_token_matches,
                    );
                }
            }

            if msg.mode.is_future() {
                hats[region].register_interest(ctx.reborrow(), msg, res);
            }

            if let RouteInterestResult::ResolvedCurrentInterest = route_interest_res {
                hats[region].send_declare_final(ctx.reborrow(), msg.id, &src);
            }
        });
    }

    #[tracing::instrument(
        level = "debug",
        name = "interest",
        skip(self, msg),
        fields(
            id = msg.id,
            mode = ?InterestMode::Final,
            opts = %msg.options,
            expr = msg.wire_expr.as_ref().map(|we| we.to_string())
        ),
        ret
    )]
    pub(crate) fn interest_final(&self, msg: &Interest) {
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let mut ctx = DispatcherContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare: &mut |_, _| unreachable!(),
        };

        // Unregister keyexpr interest
        get_mut_unchecked(ctx.src_face)
            .remote_key_interests
            .remove(&msg.id);

        let hats = &mut tables.hats;
        let region = ctx.src_face.region;

        let Some(remote_interest) = hats[region].unregister_interest(ctx.reborrow(), msg) else {
            return;
        };

        hats[Region::North].route_interest_final(ctx, msg, &remote_interest);
    }

    #[tracing::instrument(level = "debug", skip(self, wtables, _node_id, send_declare), ret)]
    pub(crate) fn declare_final(
        &self,
        wtables: &mut Tables,
        interest_id: InterestId,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let tables = &mut *wtables;

        let mut ctx = DispatcherContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare,
        };

        let hats = &mut tables.hats;
        let region = ctx.src_face.region;

        if region.bound().is_south() {
            tracing::error!("Received DeclareFinal from south-bound face");
            return;
        }

        // TODO(regions): this is too conservative, the north hat should be able to decide what
        // keyexpr(s)—if not all—are affected and whether this finalization concerns subscribers
        // or queryables or borth.
        hats[region].disable_all_routes(ctx.tables);

        match hats[region].route_declare_final(ctx.reborrow(), interest_id) {
            RouteCurrentDeclareResult::Noop | RouteCurrentDeclareResult::NoBreadcrumb => {} // ¯\_(ツ)_/¯
            RouteCurrentDeclareResult::Breadcrumb { interest } => {
                debug_assert!(interest.mode.is_current());

                hats[interest.src_region].send_declare_final(
                    ctx,
                    interest.src_interest_id,
                    &interest.src,
                );
            }
        }
    }
}
