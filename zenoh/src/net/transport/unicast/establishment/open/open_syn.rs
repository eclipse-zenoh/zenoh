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
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::message::extensions::{ZExt, ZExtPolicy};
use crate::net::protocol::message::{OpenSyn, WireProperties};
use crate::net::transport::TransportManager;

pub(super) struct Input {
    pub(super) cookie: ZSlice,
    pub(super) initial_sn: ZInt,
    pub(super) open_syn_auth_ext: WireProperties,
}

pub(super) struct Output;

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    _auth_link: &AuthenticatedPeerLink,
    input: Input,
) -> OResult<Output> {
    // Build and send an OpenSyn message
    let mut message = OpenSyn::new(manager.config.unicast.lease, input.initial_sn, input.cookie);
    if !input.open_syn_auth_ext.is_empty() {
        message.exts.authentication = Some(ZExt::new(input.open_syn_auth_ext, ZExtPolicy::Ignore));
    }

    let _ = link.send(&mut message).await.map_err(|e| (e, None))?;

    let output = Output;
    Ok(output)
}
