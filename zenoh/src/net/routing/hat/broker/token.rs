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

use std::sync::Arc;

use zenoh_protocol::network::{declare::TokenId, interest::InterestId};

use super::Hat;
use crate::net::routing::{
    dispatcher::{interests::CurrentInterest, region::RegionMap},
    hat::{BaseContext, HatTokenTrait, HatTrait},
    router::{NodeId, Resource},
};

impl HatTokenTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn declare_token(
        &mut self,
        _ctx: BaseContext,
        _id: TokenId,
        _res: &mut Arc<Resource>,
        _node_id: NodeId,
        _interest_id: Option<InterestId>,
    ) {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn undeclare_token(
        &mut self,
        _ctx: BaseContext,
        _id: TokenId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region, interest_id))]
    fn declare_current_token(
        &mut self,
        _ctx: BaseContext,
        _res: &mut Arc<Resource>,
        _interest_id: InterestId,
        _downstream_hats: RegionMap<&mut dyn HatTrait>,
    ) {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region, interest_id = interest.src_interest_id))]
    fn propagate_current_token(
        &mut self,
        _ctx: BaseContext,
        _res: &mut Arc<Resource>,
        interest: &CurrentInterest,
    ) {
        unimplemented!()
    }
}
