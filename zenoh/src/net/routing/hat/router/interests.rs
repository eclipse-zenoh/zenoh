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

use super::Hat;
use crate::net::routing::{
    dispatcher::{interests::RemoteInterest, resource::Resource, tables::TablesLock},
    hat::{CurrentFutureTrait, DeclarationContext, HatInterestTrait},
    RoutingContext,
};

impl HatInterestTrait for Hat {
    fn declare_interest(
        &self,
        ctx: DeclarationContext,
        _tables_ref: &Arc<TablesLock>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        mut options: InterestOptions,
    ) {
        if options.aggregate() && ctx.src_face.whatami == WhatAmI::Peer {
            tracing::warn!(
                "Received Interest with aggregate=true from peer {}. Not supported!",
                ctx.src_face.zid
            );
            options -= InterestOptions::AGGREGATE;
        }
        if options.subscribers() {
            self.declare_sub_interest(
                ctx.tables,
                ctx.src_face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
                ctx.send_declare,
            )
        }
        if options.queryables() {
            self.declare_qabl_interest(
                ctx.tables,
                ctx.src_face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
                ctx.send_declare,
            )
        }
        if options.tokens() {
            self.declare_token_interest(
                ctx.tables,
                ctx.src_face,
                id,
                res.as_ref().map(|r| (*r).clone()).as_mut(),
                mode,
                options.aggregate(),
                ctx.send_declare,
            )
        }
        if mode.future() {
            self.face_hat_mut(ctx.src_face).remote_interests.insert(
                id,
                RemoteInterest {
                    res: res.cloned(),
                    options,
                    mode,
                },
            );
        }
        if mode.current() {
            (ctx.send_declare)(
                &ctx.src_face.primitives,
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

    fn undeclare_interest(&self, ctx: DeclarationContext, id: InterestId) {
        self.face_hat_mut(ctx.src_face).remote_interests.remove(&id);
    }

    fn declare_final(&self, _ctx: DeclarationContext, _id: InterestId) {
        // Nothing
    }
}
