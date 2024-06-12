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
    core::WhatAmI,
    network::interest::{InterestId, InterestMode, InterestOptions},
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat_mut, pubsub::declare_sub_interest, queries::declare_qabl_interest, HatCode, HatFace,
};
use crate::net::routing::{
    dispatcher::{face::FaceState, resource::Resource, tables::Tables},
    hat::{CurrentFutureTrait, HatInterestTrait},
};

impl HatInterestTrait for HatCode {
    fn declare_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        mut options: InterestOptions,
    ) {
        if options.aggregate() && face.whatami == WhatAmI::Peer {
            tracing::warn!(
                "Received Interest with aggregate=true from peer {}. Not supported!",
                face.zid
            );
            options -= InterestOptions::AGGREGATE;
        }
        if options.subscribers() {
            declare_sub_interest(
                tables,
                face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
            )
        }
        if options.queryables() {
            declare_qabl_interest(
                tables,
                face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
            )
        }
        if mode.future() {
            face_hat_mut!(face)
                .remote_interests
                .insert(id, (res.cloned(), options));
        }
    }

    fn undeclare_interest(&self, _tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId) {
        face_hat_mut!(face).remote_interests.remove(&id);
    }
}
