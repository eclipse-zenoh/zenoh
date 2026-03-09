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

use itertools::Itertools;
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
    dispatcher::{face::Face, tables::InterRegionFilter},
    gateway::{node_id_as_source, Resource},
    hat::{DispatcherContext, RouteCurrentDeclareResult, SendDeclare},
};

impl Face {
    #[tracing::instrument(
        level = "debug",
        skip(self, tables, send_declare),
        fields(expr = %expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn declare_token(
        &self,
        tables: &TablesLock,
        id: TokenId,
        expr: &WireExpr,
        node_id: NodeId,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        if interest_id.is_some() && self.state.region.bound().is_south() {
            tracing::error!(
                src = %self.state,
                id = interest_id,
                "Received token with interest id from south-bound face. \
                This message should only flow downstream"
            );
            return;
        }

        let rtables = zread!(tables.tables);
        match rtables
            .data
            .get_mapping(&self.state, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (res, mut wtables) = if res.as_ref().map(|r| r.ctx.is_some()).unwrap_or(false) {
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

                let region = self.state.region;

                macro_rules! ctx {
                    () => {
                        DispatcherContext {
                            tables_lock: &self.tables,
                            tables: &mut tables.data,
                            src_face: &mut self.state.clone(),
                            send_declare,
                        }
                    };
                }

                let src_zid = tables.hats[region].remote_node_id_to_zid(ctx!().src_face, node_id);

                match interest_id.map(|id| {
                    tables.hats[region].route_current_token(ctx!().reborrow(), id, res.clone())
                }) {
                    Some(RouteCurrentDeclareResult::Noop) => {} // ¯\_(ツ)_/¯
                    Some(RouteCurrentDeclareResult::Breadcrumb { interest }) => {
                        if interest.mode.is_future() {
                            tables.hats[region].register_token(ctx!(), id, res.clone(), node_id);
                        }

                        tables.hats[interest.src_region].propagate_current_token(
                            ctx!(),
                            res,
                            interest,
                        );
                    }
                    Some(RouteCurrentDeclareResult::NoBreadcrumb) | None => {
                        tables.hats[region].register_token(ctx!(), id, res.clone(), node_id);

                        for dst in tables.hats.regions().collect_vec() {
                            let other_tokens = tables
                                .hats
                                .values()
                                .filter(|hat| hat.region() != dst)
                                .any(|hat| hat.remote_tokens_of(&res));

                            let filter = InterRegionFilter {
                                src: &region,
                                dst: &dst,
                                src_zid: src_zid.as_ref(),
                                fwd_zid: Some(&self.state.zid),
                                dst_zid: None,
                            };

                            if filter.resolve(tables) {
                                tables.hats[dst].propagate_token(ctx!(), res.clone(), other_tokens);
                            }
                        }
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

    #[tracing::instrument(
        level = "debug",
        skip(self, tables, send_declare),
        fields(expr = %expr.wire_expr, node_id = node_id_as_source(node_id)),
        ret
    )]
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

        let hats = &mut tables.hats;
        let region = self.state.region;

        let mut ctx = DispatcherContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut self.state.clone(),
            send_declare,
        };

        // NOTE(regions): hats[region] might not know about this token, but it still needs to be unpropagated
        if let Some(mut res) = hats[region]
            .unregister_token(ctx.reborrow(), id, res.clone(), node_id)
            .or(res)
        {
            let rem = hats
                .values_mut()
                .filter_map(|hat| hat.remote_tokens_of(&res).then_some(hat.region()))
                .collect_vec();

            tracing::trace!(?rem);

            match &*rem {
                [] => {
                    for hat in tables.hats.values_mut() {
                        hat.unpropagate_token(ctx.reborrow(), res.clone());
                    }
                    Resource::clean(&mut res);
                }
                [last_owner] if last_owner != &region => {
                    hats[last_owner].unpropagate_last_non_owned_token(ctx, res.clone())
                }
                _ => {}
            }
        }
    }
}
