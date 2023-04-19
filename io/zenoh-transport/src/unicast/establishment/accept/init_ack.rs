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
use super::{init_syn, AResult};
use crate::{
    unicast::establishment::{
        authenticator::AuthenticatedPeerLink, Cookie, EstablishmentProperties, Zenoh060Cookie,
    },
    TransportManager,
};
use rand::Rng;
use std::convert::TryFrom;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_codec::{WCodec, Zenoh060};
use zenoh_core::{zasynclock, zasyncread};
use zenoh_crypto::hmac;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    common::Attachment,
    core::Property,
    transport::{tmsg, TransportMessage},
};
use zenoh_result::zerror;

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
    let azid = manager.config.zid;
    let sn_resolution = if agreed_sn_resolution == input.sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };

    // Create the cookie
    let mut cookie = Cookie {
        whatami: input.whatami,
        zid: input.zid,
        sn_resolution: agreed_sn_resolution,
        is_qos: input.is_qos,
        nonce: zasynclock!(manager.prng).gen_range(0..agreed_sn_resolution),
        properties: EstablishmentProperties::new(),
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
                .map_err(|e| (e, Some(tmsg::close_reason::UNSUPPORTED)))?;
        }
        // Add state in the cookie if avaiable
        if let Some(cke) = cke.take() {
            ps_cookie
                .insert(Property {
                    key: pa.id().into(),
                    value: cke,
                })
                .map_err(|e| (e, Some(tmsg::close_reason::UNSUPPORTED)))?;
        }
    }
    cookie.properties = ps_cookie;

    let attachment: Option<Attachment> = if ps_attachment.is_empty() {
        None
    } else {
        let att = Attachment::try_from(&ps_attachment)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        Some(att)
    };

    let mut encrypted = vec![];
    let mut writer = encrypted.writer();
    let mut codec = Zenoh060Cookie {
        prng: &mut *zasynclock!(manager.prng),
        cipher: &manager.cipher,
        codec: Zenoh060::default(),
    };
    codec.write(&mut writer, &cookie).map_err(|_| {
        (
            zerror!("Encoding cookie failed").into(),
            Some(tmsg::close_reason::INVALID),
        )
    })?;

    // Compute the cookie hash
    let cookie_hash = hmac::digest(&encrypted);

    // Send the cookie
    let cookie: ZSlice = encrypted.into();
    let message = TransportMessage::make_init_ack(
        whatami,
        azid,
        sn_resolution,
        input.is_qos,
        cookie,
        attachment,
    );

    // Send the message on the link
    let _ = link
        .write_transport_message(&message)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    let output = Output { cookie_hash };
    Ok(output)
}
