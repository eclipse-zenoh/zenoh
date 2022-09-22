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
use crate::unicast::establishment::authenticator::AuthenticatedPeerLink;
use crate::unicast::establishment::{attachment_from_properties, EstablishmentProperties};
use crate::TransportManager;
use zenoh_core::zasyncread;
use zenoh_link::LinkUnicast;
use zenoh_protocol::core::Property;
use zenoh_protocol::proto::{tmsg, TransportMessage};

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
    let mut message = TransportMessage::make_init_syn(
        manager.config.version,
        manager.config.whatami,
        manager.config.zid,
        manager.config.sn_resolution,
        manager.config.unicast.is_qos,
        attachment_from_properties(&ps_attachment).ok(),
    );
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    let output = Output;
    Ok(output)
}
