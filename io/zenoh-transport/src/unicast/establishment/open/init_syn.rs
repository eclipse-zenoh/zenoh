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
use super::OResult;
use crate::unicast::establishment::{
    authenticator::AuthenticatedPeerLink, EstablishmentProperties,
};
use crate::TransportManager;
use std::convert::TryFrom;
use zenoh_core::zasyncread;
use zenoh_link::LinkUnicast;
use zenoh_protocol::common::Attachment;
use zenoh_protocol::{
    core::Property,
    transport::{tmsg, TransportMessage},
};

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output;

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> OResult<Output> {
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let mut att = pa
            .get_init_syn_properties(auth_link, &manager.config.zid)
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::UNSUPPORTED)))?;
        if let Some(att) = att.take() {
            ps_attachment
                .insert(Property {
                    key: pa.id().into(),
                    value: att,
                })
                .map_err(|e| (e, Some(tmsg::close_reason::UNSUPPORTED)))?;
        }
    }

    // Build and send the InitSyn message
    let init_syn_attachment = if ps_attachment.is_empty() {
        None
    } else {
        let att = Attachment::try_from(&ps_attachment)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        Some(att)
    };

    let message = TransportMessage::make_init_syn(
        manager.config.version,
        manager.config.whatami,
        manager.config.zid,
        manager.config.sn_resolution,
        manager.config.unicast.is_qos,
        init_syn_attachment,
    );
    let _ = link
        .write_transport_message(&message)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    let output = Output;
    Ok(output)
}
