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
use super::super::{
    authenticator::AuthenticatedPeerLink,
    {Cookie, EstablishmentProperties},
};
use super::AResult;
use crate::{unicast::establishment::cookie::Zenoh060Cookie, TransportManager};
use std::{convert::TryFrom, time::Duration};
use zenoh_buffers::reader::HasReader;
use zenoh_codec::{RCodec, Zenoh060};
use zenoh_core::{zasynclock, zasyncread};
use zenoh_crypto::hmac;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    common::Attachment,
    core::{Property, ZInt},
    transport::{tmsg, Close, TransportBody},
};
use zenoh_result::zerror;

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an OpenSyn
pub(super) struct Output {
    pub(super) cookie: Cookie,
    pub(super) initial_sn: ZInt,
    pub(super) lease: Duration,
    pub(super) is_shm: bool,
    pub(super) open_ack_attachment: Option<Attachment>,
}
#[allow(unused_mut)]
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &AuthenticatedPeerLink,
    input: super::init_ack::Output,
) -> AResult<Output> {
    // Wait to read an OpenSyn
    let mut messages = link
        .read_transport_message()
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
    if messages.len() != 1 {
        let e = zerror!(
            "Received multiple messages instead of a single OpenSyn on {}: {:?}",
            link,
            messages
        );
        return Err((e.into(), Some(tmsg::close_reason::INVALID)));
    }

    let mut msg = messages.remove(0);
    let open_syn = match msg.body {
        TransportBody::OpenSyn(open_syn) => open_syn,
        TransportBody::Close(Close { reason, .. }) => {
            let e = zerror!(
                "Received a close message (reason {}) instead of an OpenSyn on: {:?}",
                tmsg::close_reason_to_str(reason),
                link,
            );
            match reason {
                tmsg::close_reason::MAX_LINKS => log::debug!("{}", e),
                _ => log::error!("{}", e),
            }
            return Err((e.into(), None));
        }
        _ => {
            let e = zerror!(
                "Received invalid message instead of an OpenSyn on {}: {:?}",
                link,
                msg.body
            );
            log::error!("{}", e);
            return Err((e.into(), Some(tmsg::close_reason::INVALID)));
        }
    };
    let encrypted = open_syn.cookie.to_vec();

    // Verify that the cookie is the one we sent
    if input.cookie_hash != hmac::digest(&encrypted) {
        let e = zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link);
        return Err((e.into(), Some(tmsg::close_reason::INVALID)));
    }

    // Decrypt the cookie with the cyper
    let mut reader = encrypted.reader();
    let mut codec = Zenoh060Cookie {
        prng: &mut *zasynclock!(manager.prng),
        cipher: &manager.cipher,
        codec: Zenoh060::default(),
    };
    let mut cookie: Cookie = codec.read(&mut reader).map_err(|_| {
        (
            zerror!("Decoding cookie failed").into(),
            Some(tmsg::close_reason::INVALID),
        )
    })?;

    // Validate with the peer authenticators
    let mut open_syn_properties: EstablishmentProperties = match msg.attachment.take() {
        Some(att) => EstablishmentProperties::try_from(&att)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?,
        None => EstablishmentProperties::new(),
    };

    let mut is_shm = false;
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let po = open_syn_properties.remove(pa.id().into()).map(|x| x.value);
        let pc = cookie.properties.remove(pa.id().into()).map(|x| x.value);
        let mut att = pa.handle_open_syn(auth_link, &cookie, (po, pc)).await;

        #[cfg(feature = "shared-memory")]
        {
            if pa.id() == super::super::authenticator::PeerAuthenticatorId::Shm {
                // Check if SHM has been validated from the other side
                att = match att {
                    Ok(att) => {
                        is_shm = true;
                        Ok(att)
                    }
                    Err(e) => {
                        if e.is::<zenoh_result::ShmError>() {
                            is_shm = false;
                            Ok(None)
                        } else {
                            Err(e)
                        }
                    }
                };
            }
        }

        let mut att = att.map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        if let Some(att) = att.take() {
            ps_attachment
                .insert(Property {
                    key: pa.id().into(),
                    value: att,
                })
                .map_err(|e| (e, Some(tmsg::close_reason::UNSUPPORTED)))?;
        }
    }

    let open_ack_attachment = if ps_attachment.is_empty() {
        None
    } else {
        let att = Attachment::try_from(&ps_attachment)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        Some(att)
    };

    let output = Output {
        cookie,
        initial_sn: open_syn.initial_sn,
        lease: open_syn.lease,
        is_shm,
        open_ack_attachment,
    };
    Ok(output)
}
