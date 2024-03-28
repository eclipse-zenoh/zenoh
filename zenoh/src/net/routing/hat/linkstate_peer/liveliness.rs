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

    fn declare_liveliness_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
        res: Option<&mut Arc<Resource>>,
        current: bool,
        future: bool,
        aggregate: bool,
    ) {
        todo!()
    }

    fn undeclare_liveliness_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
    ) {
        todo!()
    }
}
