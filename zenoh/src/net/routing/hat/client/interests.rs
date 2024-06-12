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
use std::sync::{atomic::Ordering, Arc};

use zenoh_protocol::{
    core::WhatAmI,
    network::{
        declare::ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        resource::Resource,
        tables::Tables,
    },
    hat::HatInterestTrait,
    RoutingContext,
};

pub(super) fn interests_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    for mut src_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        if face.whatami != WhatAmI::Client {
            for (res, options) in face_hat_mut!(&mut src_face).remote_interests.values() {
                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                get_mut_unchecked(face).local_interests.insert(
                    id,
                    InterestState {
                        options: *options,
                        res: res.as_ref().map(|res| (*res).clone()),
                        finalized: false,
                    },
                );
                let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, face));
                face.primitives.send_interest(RoutingContext::with_expr(
                    Interest {
                        id,
                        mode: InterestMode::CurrentFuture,
                        options: *options,
                        wire_expr,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                    },
                    res.as_ref().map(|res| res.expr()).unwrap_or_default(),
                ));
            }
        }
    }
}

impl HatInterestTrait for HatCode {
    fn declare_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        options: InterestOptions,
    ) {
        face_hat_mut!(face)
            .remote_interests
            .insert(id, (res.as_ref().map(|res| (*res).clone()), options));
        for dst_face in tables
            .faces
            .values_mut()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            get_mut_unchecked(dst_face).local_interests.insert(
                id,
                InterestState {
                    options,
                    res: res.as_ref().map(|res| (*res).clone()),
                    finalized: mode == InterestMode::Future,
                },
            );
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, dst_face));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                Interest {
                    id,
                    mode,
                    options,
                    wire_expr,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                },
                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
            ));
        }
    }

    fn undeclare_interest(&self, tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId) {
        if let Some(interest) = face_hat_mut!(face).remote_interests.remove(&id) {
            if !tables.faces.values().any(|f| {
                f.whatami == WhatAmI::Client
                    && face_hat!(f)
                        .remote_interests
                        .values()
                        .any(|i| *i == interest)
            }) {
                for dst_face in tables
                    .faces
                    .values_mut()
                    .filter(|f| f.whatami != WhatAmI::Client)
                {
                    for id in dst_face
                        .local_interests
                        .keys()
                        .cloned()
                        .collect::<Vec<InterestId>>()
                    {
                        let local_interest = dst_face.local_interests.get(&id).unwrap();
                        if local_interest.res == interest.0 {
                            dst_face.primitives.send_interest(RoutingContext::with_expr(
                                Interest {
                                    id,
                                    mode: InterestMode::Final,
                                    options: InterestOptions::empty(),
                                    wire_expr: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                },
                                local_interest
                                    .res
                                    .as_ref()
                                    .map(|res| res.expr())
                                    .unwrap_or_default(),
                            ));
                            get_mut_unchecked(dst_face).local_interests.remove(&id);
                        }
                    }
                }
            }
        }
    }
}
