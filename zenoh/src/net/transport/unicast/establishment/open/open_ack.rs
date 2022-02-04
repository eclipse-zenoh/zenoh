//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::super::authenticator::AuthenticatedPeerLink;
use super::OResult;
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::ZInt;
use crate::net::protocol::message::{CloseReason, OpenAck, WireProperties};
use crate::net::transport::TransportManager;
use std::time::Duration;

pub(super) struct Output {
    pub(super) initial_sn: ZInt,
    pub(super) lease: Duration,
}

pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &AuthenticatedPeerLink,
    _input: super::open_syn::Output,
) -> OResult<Output> {
    // Wait to read an OpenAck
    let mut open_ack: OpenAck = link.recv().await.map_err(|e| (e, None))?;

    let mut open_ack_auth_ext: WireProperties = match open_ack.exts.authentication.take() {
        Some(auth) => auth.into_inner().into(),
        None => WireProperties::new(),
    };
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let _ = pa
            .handle_open_ack(auth_link, open_ack_auth_ext.remove(&pa.id().into()))
            .await
            .map_err(|e| (e, Some(CloseReason::Invalid)))?;
    }

    let output = Output {
        initial_sn: open_ack.initial_sn,
        lease: open_ack.lease,
    };
    Ok(output)
}
