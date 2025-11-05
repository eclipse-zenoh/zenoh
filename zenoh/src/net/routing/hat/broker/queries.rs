//
// Copyright (c) 2025 ZettaScale Technology
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
use std::{collections::HashMap, sync::Arc};

use zenoh_protocol::network::declare::{queryable::ext::QueryableInfoType, QueryableId};

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            resource::{NodeId, Resource},
            tables::{QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatQueriesTrait, Sources},
    },
};

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl HatQueriesTrait for Hat {
    fn declare_queryable(
        &mut self,
        _ctx: BaseContext,
        _id: QueryableId,
        _res: &mut Arc<Resource>,
        _node_id: NodeId,
        _qabl_info: &QueryableInfoType,
    ) {
        unimplemented!()
    }

    fn undeclare_queryable(
        &mut self,
        _ctx: BaseContext,
        _id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        unimplemented!()
    }

    fn get_queryables(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
    }

    fn get_queriers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(expr = ?expr, rgn = %self.region))]
    fn compute_query_route(
        &self,
        _tables: &TablesData,
        _src_face: &FaceState,
        expr: &RoutingExpr,
        _source: NodeId,
    ) -> Arc<QueryTargetQablSet> {
        unimplemented!()
    }

    fn get_matching_queryables(
        &self,
        _tables: &TablesData,
        _key_expr: &KeyExpr<'_>,
        _complete: bool,
    ) -> HashMap<usize, Arc<FaceState>> {
        unimplemented!()
    }
}
