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
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::WireExpr,
    network::{
        declare::ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal,
    },
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

use super::{
    face::FaceState,
    tables::{register_expr_interest, Tables, TablesLock},
};
use crate::net::routing::{
    hat::{HatTrait, SendDeclare},
    router::{unregister_expr_interest, Resource},
    RoutingContext,
};

pub(crate) struct CurrentInterest {
    pub(crate) src_face: Arc<FaceState>,
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

pub(crate) fn declare_final(
    hat_code: &(dyn HatTrait + Send + Sync),
    wtables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: InterestId,
    send_declare: &mut SendDeclare,
) {
    if let Some(interest) = get_mut_unchecked(face)
        .pending_current_interests
        .remove(&id)
    {
        finalize_pending_interest(interest, send_declare);
    }

    hat_code.declare_final(wtables, face, id);
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
        tracing::debug!(
            "{}:{} Propagate DeclareFinal",
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
                        "{}:{} Didn't receive DeclareFinal for interest {}:{}: Timeout({:#?})!",
                        face,
                        self.id,
                        interest.interest.src_face,
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn declare_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    id: InterestId,
    expr: Option<&WireExpr>,
    mode: InterestMode,
    options: InterestOptions,
    send_declare: &mut SendDeclare,
) {
    if options.keyexprs() && mode != InterestMode::Current {
        register_expr_interest(tables_ref, face, id, expr);
    }

    if let Some(expr) = expr {
        let rtables = zread!(tables_ref.tables);
        match rtables
            .get_mapping(face, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                tracing::debug!(
                    "{} Declare interest {} ({}{})",
                    face,
                    id,
                    prefix.expr(),
                    expr.suffix
                );
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) =
                    if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                        drop(rtables);
                        let wtables = zwrite!(tables_ref.tables);
                        (res.unwrap(), wtables)
                    } else {
                        let mut fullexpr = prefix.expr().to_string();
                        fullexpr.push_str(expr.suffix.as_ref());
                        let mut matches = keyexpr::new(fullexpr.as_str())
                            .map(|ke| Resource::get_matches(&rtables, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut wtables = zwrite!(tables_ref.tables);
                        let mut res = Resource::make_resource(
                            hat_code,
                            &mut wtables,
                            &mut prefix,
                            expr.suffix.as_ref(),
                        );
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&wtables, &mut res, matches);
                        (res, wtables)
                    };

                hat_code.declare_interest(
                    &mut wtables,
                    tables_ref,
                    face,
                    id,
                    Some(&mut res),
                    mode,
                    options,
                    send_declare,
                );
            }
            None => tracing::error!(
                "{} Declare interest {} for unknown scope {}!",
                face,
                id,
                expr.scope
            ),
        }
    } else {
        let mut wtables = zwrite!(tables_ref.tables);
        hat_code.declare_interest(
            &mut wtables,
            tables_ref,
            face,
            id,
            None,
            mode,
            options,
            send_declare,
        );
    }
}

pub(crate) fn undeclare_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
) {
    tracing::debug!("{} Undeclare interest {}", face, id,);
    unregister_expr_interest(tables, face, id);
    let mut wtables = zwrite!(tables.tables);
    hat_code.undeclare_interest(&mut wtables, face, id);
}
