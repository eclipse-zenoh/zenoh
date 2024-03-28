//
// Copyright (c) 2023 ZettaScale Technology
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
    network::declare::{common::ext, InterestId, TokenId},
};

use crate::net::routing::{hat::HatTrait, router::Resource};

use super::{
    face::FaceState,
    tables::{NodeId, TablesLock},
};

pub(crate) fn declare_liveliness(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: TokenId,
    expr: &WireExpr,
    node_id: NodeId,
) {
    let rtables = zread!(tables.tables);
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            log::debug!(
                "{} Declare liveliness {} ({}{})",
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
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };

            hat_code.declare_liveliness(&mut wtables, face, id, &mut res, node_id);
            drop(wtables);

            // NOTE(fuzzypixelz): I removed all data route handling.
        }
        None => log::error!(
            "{} Declare liveliness {} for unknown scope {}!",
            face,
            id,
            expr.scope
        ),
    }
}

pub(crate) fn undeclare_liveliness(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: TokenId,
    expr: &ext::WireExprType,
    node_id: NodeId,
) {
    let res = if expr.wire_expr.is_empty() {
        None
    } else {
        let rtables = zread!(tables.tables);
        match rtables.get_mapping(face, &expr.wire_expr.scope, expr.wire_expr.mapping) {
            Some(prefix) => match Resource::get_resource(prefix, expr.wire_expr.suffix.as_ref()) {
                Some(res) => Some(res),
                None => {
                    log::error!(
                        "{} Undeclare unknown liveliness token {}{}!",
                        face,
                        prefix.expr(),
                        expr.wire_expr.suffix
                    );
                    return;
                }
            },
            None => {
                log::error!(
                    "{} Undeclare liveliness token with unknown scope {}",
                    face,
                    expr.wire_expr.scope
                );
                return;
            }
        }
    };

    let mut wtables = zwrite!(tables.tables);
    hat_code.undeclare_liveliness(&mut wtables, face, id, res, node_id);

    // NOTE(fuzzypixelz): I removed all data route handling.
    // NOTE(fuzzypixelz): Unlike in `undeclare_liveliness` this doesn't return the ressource.
}

#[allow(clippy::too_many_arguments)] // TODO refactor
pub(crate) fn declare_liveliness_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
    expr: Option<&WireExpr>,
    current: bool,
    future: bool,
    aggregate: bool,
) {
    if let Some(expr) = expr {
        let rtables = zread!(tables.tables);
        match rtables
            .get_mapping(face, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                log::debug!(
                    "{} Declare liveliness interest {} ({}{})",
                    face,
                    id,
                    prefix.expr(),
                    expr.suffix
                );
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) = if res
                    .as_ref()
                    .map(|r| r.context.is_some())
                    .unwrap_or(false)
                {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };

                hat_code.declare_liveliness_interest(
                    &mut wtables,
                    face,
                    id,
                    Some(&mut res),
                    current,
                    future,
                    aggregate,
                );
            }
            None => log::error!(
                "{} Declare liveliness interest {} for unknown scope {}!",
                face,
                id,
                expr.scope
            ),
        }
    } else {
        let mut wtables = zwrite!(tables.tables);
        hat_code.declare_liveliness_interest(
            &mut wtables,
            face,
            id,
            None,
            current,
            future,
            aggregate,
        );
    }
}

pub(crate) fn undeclare_liveliness_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
) {
    log::debug!("{} Undeclare liveliness interest {}", face, id,);
    let mut wtables = zwrite!(tables.tables);
    hat_code.undeclare_liveliness_interest(&mut wtables, face, id);
}
