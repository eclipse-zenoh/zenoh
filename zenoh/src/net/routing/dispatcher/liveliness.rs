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

use zenoh_protocol::{
    core::WireExpr,
    network::declare::{common::ext, TokenId},
};

use crate::net::routing::hat::HatTrait;

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
    todo!()
}

pub(crate) fn undeclare_liveliness(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: TokenId,
    expr: &ext::WireExprType,
    node_id: NodeId,
) {
    todo!()
}
