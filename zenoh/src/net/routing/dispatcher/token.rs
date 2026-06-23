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

use itertools::Itertools;
use zenoh_protocol::{
    core::WireExpr,
    network::{
        declare::{common::ext, TokenId},
        interest::InterestId,
    },
};

use super::tables::NodeId;
use crate::net::routing::{
    dispatcher::{face::Face, tables::InterRegionFilter},
    gateway::{node_id_as_source, Resource},
    hat::{DispatcherContext, RouteCurrentDeclareResult, SendDeclare},
};

impl Face {
    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare),
        fields(expr = %expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn declare_token(
        &self,
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

        self.with_mapped_expr(expr, |tables, res| {
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

            let mut ctx = ctx!();

            let src_zid = tables.hats[region].remote_node_id_to_zid(ctx.src_face, node_id);

            match interest_id
                .map(|id| tables.hats[region].route_current_token(ctx.reborrow(), id, res.clone()))
            {
                Some(RouteCurrentDeclareResult::Noop) => {} // ¯\_(ツ)_/¯
                Some(RouteCurrentDeclareResult::Breadcrumb { interest }) => {
                    if interest.mode.is_future() {
                        tables.hats[region].register_token(
                            ctx.reborrow(),
                            id,
                            res.clone(),
                            node_id,
                        );
                    }

                    tables.hats[interest.src_region].propagate_current_token(ctx, res, interest);
                }
                Some(RouteCurrentDeclareResult::NoBreadcrumb) | None => {
                    tables.hats[region].register_token(ctx, id, res.clone(), node_id);

                    for dst in tables.hats.regions().collect_vec() {
                        let filter = InterRegionFilter {
                            src: &region,
                            dst: &dst,
                            src_zid: src_zid.as_ref(),
                            fwd_zid: Some(&self.state.zid),
                            dst_zid: None,
                        };

                        if filter.resolve(tables) {
                            debug_assert_implies!(
                                !tables
                                    .hats
                                    .values()
                                    .filter(|hat| hat.region() != dst)
                                    .any(|hat| hat.remote_tokens_of(&tables.data, &res)),
                                tables.hats[dst].owns(&self.state)
                            );
                            tables.hats[dst].propagate_token(ctx!(), res.clone());
                        }
                    }
                }
            }
        });
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare),
        fields(expr = %expr.wire_expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn undeclare_token(
        &self,
        id: TokenId,
        expr: &ext::WireExprType,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        // TODO: here we create a Resource that will immediately be removed after treatment, this
        // could be improved
        self.with_mapped_nullable_expr(
            &expr.wire_expr,
            /* make_if_unknown */ true,
            |tables, res| {
                let region = self.state.region;

                let mut ctx = DispatcherContext {
                    tables_lock: &self.tables,
                    tables: &mut tables.data,
                    src_face: &mut self.state.clone(),
                    send_declare,
                };

                // NOTE(regions): tables.hats[region] might not know about this token, but it still
                // needs to be unpropagated
                if let Some(mut res) = tables.hats[region]
                    .unregister_token(ctx.reborrow(), id, res.clone(), node_id)
                    .or(res)
                {
                    let rem = tables
                        .hats
                        .values_mut()
                        .filter_map(|hat| {
                            hat.remote_tokens_of(ctx.tables, &res)
                                .then_some(hat.region())
                        })
                        .collect_vec();

                    tracing::trace!(?rem);

                    match &*rem {
                        [] => {
                            for hat in tables.hats.values_mut() {
                                hat.unpropagate_token(ctx.reborrow(), res.clone());
                            }
                            Resource::clean(&mut res);
                        }
                        [last_owner] if last_owner != &region => tables.hats[last_owner]
                            .unpropagate_last_non_owned_token(ctx, res.clone()),
                        _ => {}
                    }
                }
            },
        );
    }
}
