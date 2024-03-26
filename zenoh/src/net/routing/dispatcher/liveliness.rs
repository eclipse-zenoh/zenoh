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
