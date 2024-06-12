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
    network::{
        declare::ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal,
    },
};

use super::{
    face::FaceState,
    tables::{register_expr_interest, TablesLock},
};
use crate::net::routing::{
    hat::HatTrait,
    router::{unregister_expr_interest, Resource},
    RoutingContext,
};

pub(crate) fn declare_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
    expr: Option<&WireExpr>,
    mode: InterestMode,
    options: InterestOptions,
) {
    if options.keyexprs() && mode != InterestMode::Current {
        register_expr_interest(tables, face, id, expr);
    }

    if let Some(expr) = expr {
        let rtables = zread!(tables.tables);
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

                hat_code.declare_interest(&mut wtables, face, id, Some(&mut res), mode, options);
            }
            None => tracing::error!(
                "{} Declare interest {} for unknown scope {}!",
                face,
                id,
                expr.scope
            ),
        }
    } else {
        let mut wtables = zwrite!(tables.tables);
        hat_code.declare_interest(&mut wtables, face, id, None, mode, options);
    }

    if mode != InterestMode::Future {
        face.primitives.send_declare(RoutingContext::new(Declare {
            interest_id: Some(id),
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareFinal(DeclareFinal),
        }));
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
