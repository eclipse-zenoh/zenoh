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

use super::{
    face::FaceState,
    tables::{NodeId, TablesLock},
};
use crate::net::routing::{
    hat::{HatTrait, SendDeclare},
    router::Resource,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn declare_token(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: TokenId,
    expr: &WireExpr,
    node_id: NodeId,
    interest_id: Option<InterestId>,
    send_declare: &mut SendDeclare,
) {
    let rtables = zread!(tables.tables);
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            tracing::debug!(
                "{} Declare token {} ({}{})",
                face,
                id,
                prefix.expr(),
                expr.suffix
            );
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr().to_string();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
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

            hat_code.declare_token(
                &mut wtables,
                face,
                id,
                &mut res,
                node_id,
                interest_id,
                send_declare,
            );
            drop(wtables);
        }
        None => tracing::error!(
            "{} Declare token {} for unknown scope {}!",
            face,
            id,
            expr.scope
        ),
    }
}

pub(crate) fn undeclare_token(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
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
            .get_mapping(face, &expr.wire_expr.scope, expr.wire_expr.mapping)
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
                            .map(|ke| Resource::get_matches(&rtables, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut wtables = zwrite!(tables.tables);
                        let mut res = Resource::make_resource(
                            hat_code,
                            &mut wtables,
                            &mut prefix,
                            expr.wire_expr.suffix.as_ref(),
                        );
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&wtables, &mut res, matches);
                        (Some(res), wtables)
                    }
                }
            }
            None => {
                tracing::error!(
                    "{} Undeclare liveliness token with unknown scope {}",
                    face,
                    expr.wire_expr.scope
                );
                return;
            }
        }
    };

    if let Some(res) = hat_code.undeclare_token(&mut wtables, face, id, res, node_id, send_declare)
    {
        tracing::debug!("{} Undeclare token {} ({})", face, id, res.expr());
    } else {
        // NOTE: This is expected behavior if liveliness tokens are denied with ingress ACL interceptor.
        tracing::debug!("{} Undeclare unknown token {}", face, id);
    }
}
