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

use zenoh_protocol::{
    core::WhatAmI,
    network::{
        declare::ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat_mut, pubsub::declare_sub_interest, queries::declare_qabl_interest,
    token::declare_token_interest, HatCode, HatFace,
};
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        interests::RemoteInterest,
        resource::Resource,
        tables::{Tables, TablesLock},
    },
    hat::{CurrentFutureTrait, HatInterestTrait, SendDeclare},
    RoutingContext,
};

impl HatInterestTrait for HatCode {
    fn declare_interest(
        &self,
        tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        mut options: InterestOptions,
        send_declare: &mut SendDeclare,
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
                send_declare,
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
                send_declare,
            )
        }
        if options.tokens() {
            declare_token_interest(
                tables,
                face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
                send_declare,
            )
        }
        if mode.future() {
            face_hat_mut!(face).remote_interests.insert(
                id,
                RemoteInterest {
                    res: res.cloned(),
                    options,
                    mode,
                },
            );
        }
        if mode.current() {
            send_declare(
                &face.primitives,
                RoutingContext::new(Declare {
                    interest_id: Some(id),
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                }),
            );
        }
    }

    fn undeclare_interest(&self, _tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId) {
        face_hat_mut!(face).remote_interests.remove(&id);
    }

    fn declare_final(&self, _tables: &mut Tables, _face: &mut Arc<FaceState>, _id: InterestId) {
        // Nothing
    }
}
