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
use super::{
    attachment_from_properties, init_syn, AResult, AuthenticatedPeerLink, Cookie,
    EstablishmentProperties,
};
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::Property;
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::proto::{tmsg, TransportMessage};
use crate::net::transport::TransportManager;
use rand::Rng;
use zenoh_core::zasynclock;
use zenoh_util::crypto::hmac;

// Send an InitAck
pub(super) struct Output {
    pub(super) cookie_hash: Vec<u8>,
}

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &AuthenticatedPeerLink,
    mut input: init_syn::Output,
) -> AResult<Output> {
    // Compute the minimum SN Resolution
    let agreed_sn_resolution = manager.config.sn_resolution.min(input.sn_resolution);

    // Build the fields for the InitAck message
    let whatami = manager.config.whatami;
    let apid = manager.config.pid;
    let sn_resolution = if agreed_sn_resolution == input.sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };

    // Create the cookie
    let cookie = Cookie {
        whatami: input.whatami,
        pid: input.pid,
        sn_resolution: agreed_sn_resolution,
        is_qos: input.is_qos,
        nonce: zasynclock!(manager.prng).gen_range(0..agreed_sn_resolution),
    };

    // Build the attachment from the authenticators
    let mut ps_attachment = EstablishmentProperties::new();
    let mut ps_cookie = EstablishmentProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let (mut att, mut cke) = pa
            .handle_init_syn(
                auth_link,
                &cookie,
                input
                    .init_syn_properties
                    .remove(pa.id().into())
                    .map(|x| x.value),
            )
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        // Add attachment property if available
        if let Some(att) = att.take() {
            ps_attachment
                .insert(Property {
                    key: pa.id().into(),
                    value: att,
                })
                .map_err(|e| (e, None))?;
        }
        // Add state in the cookie if avaiable
        if let Some(cke) = cke.take() {
            ps_cookie
                .insert(Property {
                    key: pa.id().into(),
                    value: cke,
                })
                .map_err(|e| (e, None))?;
        }
    }
    let attachment = attachment_from_properties(&ps_attachment).ok();

    let encrypted = cookie
        .encrypt(&manager.cipher, &mut *zasynclock!(manager.prng), ps_cookie)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Compute the cookie hash
    let cookie_hash = hmac::digest(&encrypted);

    // Send the cookie
    let cookie: ZSlice = encrypted.into();
    let mut message = TransportMessage::make_init_ack(
        whatami,
        apid,
        sn_resolution,
        input.is_qos,
        cookie,
        attachment,
    );

    // Send the message on the link
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, None))?;

    let output = Output { cookie_hash };
    Ok(output)
}
