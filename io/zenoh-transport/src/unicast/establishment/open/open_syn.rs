//
// Copyright (c) 2022 ZettaScale Technology
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
use super::OResult;
use crate::TransportManager;
use zenoh_buffers::ZSlice;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::{Resolution, ZInt},
    transport::{close, OpenSyn, TransportMessage},
};

pub(super) struct Input {
    pub(super) cookie: ZSlice,
    pub(super) resolution: Resolution,
    pub(super) initial_sn: ZInt,
}

pub(super) struct Output {
    pub(super) resolution: Resolution,
}

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: Input,
) -> OResult<Output> {
    // Build and send an OpenSyn message
    let lease = manager.config.unicast.lease;
    let message: TransportMessage = OpenSyn {
        lease,
        initial_sn: input.initial_sn,
        cookie: input.cookie,
        shm: None,  // @TODO
        auth: None, // @TODO
    }
    .into();

    let _ = link
        .write_transport_message(&message)
        .await
        .map_err(|e| (e, Some(close::reason::GENERIC)))?;

    let output = Output {
        resolution: input.resolution,
    };
    Ok(output)
}
