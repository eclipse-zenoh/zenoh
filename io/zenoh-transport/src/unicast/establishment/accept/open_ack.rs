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
use super::super::AuthenticatedPeerLink;
use super::AResult;
use crate::TransportManager;
use zenoh_link::LinkUnicast;
use zenoh_protocol::core::ZInt;
use zenoh_protocol::proto::{tmsg, Attachment, TransportMessage};

/*************************************/
/*             ACCEPT                */
/*************************************/
// Send an OpenAck
pub(super) struct Input {
    pub(super) initial_sn: ZInt,
    pub(super) attachment: Option<Attachment>,
}

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    _auth_link: &AuthenticatedPeerLink,
    input: Input,
) -> AResult<()> {
    // Build OpenAck message
    let mut message = TransportMessage::make_open_ack(
        manager.config.unicast.lease,
        input.initial_sn,
        input.attachment,
    );

    // Send the message on the link
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    Ok(())
}
