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

use std::sync::Arc;

use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::WireExpr,
    network::{
        declare::{common::ext, TokenId},
        interest::InterestId,
    },
};

use super::tables::{NodeId, TablesLock};
use crate::net::routing::{
    dispatcher::face::Face,
    hat::{BaseContext, HatTrait, InterestProfile, SendDeclare},
    router::Resource,
};

impl Face {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_token(
        &self,
        tables: &TablesLock,
        id: TokenId,
        expr: &WireExpr,
        node_id: NodeId,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        let rtables = zread!(tables.tables);
        match rtables
            .data
            .get_mapping(&self.state, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                tracing::debug!(
                    "{} Declare token {} ({}{})",
                    &self.state,
                    id,
                    prefix.expr(),
                    expr.suffix
                );
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) =
                    if res.as_ref().map(|r| r.ctx.is_some()).unwrap_or(false) {
                        drop(rtables);
                        let wtables = zwrite!(tables.tables);
                        (res.unwrap(), wtables)
                    } else {
                        let mut fullexpr = prefix.expr().to_string();
                        fullexpr.push_str(expr.suffix.as_ref());
                        let mut matches = keyexpr::new(fullexpr.as_str())
                            .map(|ke| Resource::get_matches(&rtables.data, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut tables_wguard = zwrite!(tables.tables);
                        let tables = &mut *tables_wguard;
                        let mut res =
                            Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&tables.data, &mut res, matches);
                        (res, tables_wguard)
                    };

                let tables = &mut *wtables;

                if let Some(interest_id) = interest_id {
                    if !self.state.bound.is_north() {
                        tracing::error!(
                            id,
                            "Received current token from south/eastwest-bound face. \
                            This message should only flow downstream"
                        );
                        return;
                    }

                    let (upstream_hat, downstream_hats) = tables.hats.partition_north_mut();

                    upstream_hat.declare_current_token(
                        BaseContext {
                            tables_lock: &self.tables,
                            tables: &mut tables.data,
                            src_face: &mut self.state.clone(),
                            send_declare,
                        },
                        id,
                        &mut res,
                        node_id,
                        interest_id,
                        downstream_hats
                            .into_iter()
                            .map(|(b, d)| (b, &mut **d as &mut dyn HatTrait))
                            .collect(),
                    );
                } else {
                    for (bound, hat) in tables.hats.iter_mut() {
                        hat.declare_token(
                            BaseContext {
                                tables_lock: &self.tables,
                                tables: &mut tables.data,
                                src_face: &mut self.state.clone(),
                                send_declare,
                            },
                            id,
                            &mut res,
                            node_id,
                            interest_id,
                            InterestProfile::with_bound_flow((&self.state.bound, bound)),
                        );
                    }
                }
            }
            None => tracing::error!(
                "{} Declare token {} for unknown scope {}!",
                &self.state,
                id,
                expr.scope
            ),
        }
    }

    pub(crate) fn undeclare_token(
        &self,
        tables: &TablesLock,
        id: TokenId,
        expr: &ext::WireExprType,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let (res, mut wtables) = if expr.wire_expr.is_empty() {
            (None, zwrite!(tables.tables))
        } else {
            let rtables = zread!(tables.tables);
            match rtables
                .data
                .get_mapping(&self.state, &expr.wire_expr.scope, expr.wire_expr.mapping)
                .cloned()
            {
                Some(mut prefix) => {
                    match Resource::get_resource(&prefix, expr.wire_expr.suffix.as_ref()) {
                        Some(res) => {
                            drop(rtables);
                            (Some(res), zwrite!(tables.tables))
                        }
                        None => {
                            // Here we create a Resource that will immediately be removed after treatment
                            // TODO this could be improved
                            let mut fullexpr = prefix.expr().to_string();
                            fullexpr.push_str(expr.wire_expr.suffix.as_ref());
                            let mut matches = keyexpr::new(fullexpr.as_str())
                                .map(|ke| Resource::get_matches(&rtables.data, ke))
                                .unwrap_or_default();
                            drop(rtables);
                            let mut wtables = zwrite!(tables.tables);
                            let tables = &mut *wtables;
                            let mut res = Resource::make_resource(
                                tables,
                                &mut prefix,
                                expr.wire_expr.suffix.as_ref(),
                            );
                            matches.push(Arc::downgrade(&res));
                            Resource::match_resource(&tables.data, &mut res, matches);
                            (Some(res), wtables)
                        }
                    }
                }
                None => {
                    tracing::error!(
                        "{} Undeclare liveliness token with unknown scope {}",
                        &self.state,
                        expr.wire_expr.scope
                    );
                    return;
                }
            }
        };

        let tables = &mut *wtables;

        tracing::trace!(?self.state.bound);

        let res_cleanup = tables.hats.iter_mut().filter_map(|(bound, hat)| {
            let res = hat.undeclare_token(
                BaseContext {
                    tables_lock: &self.tables,
                    tables: &mut tables.data,
                    src_face: &mut self.state.clone(),
                    send_declare,
                },
                id,
                res.clone(),
                node_id,
                InterestProfile::with_bound_flow((&self.state.bound, bound)),
            );

            match res {
                Some(res) => {
                    tracing::debug!("{} Undeclare token {} ({})", &self.state, id, res.expr());
                    Some(res)
                }
                None => {
                    // NOTE: This is expected behavior if liveliness tokens are denied with ingress ACL interceptor.
                    tracing::debug!("{} Undeclare unknown token {}", &self.state, id);
                    None
                }
            }
        });

        // REVIEW(regions): this is necessary if HatFace is global
        for mut res in res_cleanup {
            Resource::clean(&mut res);
        }
    }
}
