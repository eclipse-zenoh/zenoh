use std::sync::Arc;

use zenoh_protocol::network::declare::TokenId;

use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::HatLivelinessTrait,
    router::{NodeId, Resource},
};

use super::HatCode;

impl HatLivelinessTrait for HatCode {
    fn declare_liveliness(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
    ) {
        todo!()
    }

    fn undeclare_liveliness(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) {
        todo!()
    }
}
