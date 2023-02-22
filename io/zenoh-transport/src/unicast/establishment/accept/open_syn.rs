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
use super::AResult;
use crate::{
    unicast::establishment::cookie::{Cookie, Zenoh080Cookie},
    TransportManager,
};
use std::time::Duration;
use zenoh_buffers::reader::HasReader;
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::zasynclock;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::ZInt,
    transport::{
        close::{self, Close},
        TransportBody,
    },
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
}
#[allow(unused_mut)]
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: super::init_ack::Output,
) -> AResult<Output> {
    // Wait to read an OpenSyn
    let mut messages = link
        .read_transport_message()
        .await
        .map_err(|e| (e, Some(close::reason::INVALID)))?;
    if messages.len() != 1 {
        let e = zerror!(
            "Received multiple messages instead of a single OpenSyn on {}: {:?}",
            link,
            messages
        );
        return Err((e.into(), Some(close::reason::INVALID)));
    }

    let mut msg = messages.remove(0);
    let open_syn = match msg.body {
        TransportBody::OpenSyn(open_syn) => open_syn,
        TransportBody::Close(Close { reason, .. }) => {
            let e = zerror!(
                "Received a close message (reason {}) instead of an OpenSyn on: {:?}",
                close::reason_to_str(reason),
                link,
            );
            match reason {
                close::reason::MAX_LINKS => log::debug!("{}", e),
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
            return Err((e.into(), Some(close::reason::INVALID)));
        }
    };
    let encrypted = open_syn.cookie.to_vec();

    // Decrypt the cookie with the cipher
    let mut reader = encrypted.reader();
    let mut codec = Zenoh080Cookie {
        prng: &mut *zasynclock!(manager.prng),
        cipher: &manager.cipher,
        codec: Zenoh080::default(),
    };
    let mut cookie: Cookie = codec.read(&mut reader).map_err(|_| {
        (
            zerror!("Decoding cookie failed").into(),
            Some(close::reason::INVALID),
        )
    })?;

    // Verify that the cookie is the one we sent
    if input.nonce != cookie.nonce {
        let e = zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link);
        return Err((e.into(), Some(close::reason::INVALID)));
    }

    // // Validate with the peer authenticators
    // let mut open_syn_properties: EstablishmentProperties = match msg.attachment.take() {
    //     Some(att) => EstablishmentProperties::try_from(&att)
    //         .map_err(|e| (e, Some(close::reason::INVALID)))?,
    //     None => EstablishmentProperties::new(),
    // };

    // let mut is_shm = false;
    // let mut ps_attachment = EstablishmentProperties::new();
    // for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
    //     let po = open_syn_properties.remove(pa.id().into()).map(|x| x.value);
    //     let pc = cookie.properties.remove(pa.id().into()).map(|x| x.value);
    //     let mut att = pa.handle_open_syn(auth_link, &cookie, (po, pc)).await;

    //     #[cfg(feature = "shared-memory")]
    //     {
    //         if pa.id() == super::super::authenticator::PeerAuthenticatorId::Shm {
    //             // Check if SHM has been validated from the other side
    //             att = match att {
    //                 Ok(att) => {
    //                     is_shm = true;
    //                     Ok(att)
    //                 }
    //                 Err(e) => {
    //                     if e.is::<zenoh_result::ShmError>() {
    //                         is_shm = false;
    //                         Ok(None)
    //                     } else {
    //                         Err(e)
    //                     }
    //                 }
    //             };
    //         }
    //     }

    //     let mut att = att.map_err(|e| (e, Some(close::reason::INVALID)))?;
    //     if let Some(att) = att.take() {
    //         ps_attachment
    //             .insert(Property {
    //                 key: pa.id().into(),
    //                 value: att,
    //             })
    //             .map_err(|e| (e, Some(close::reason::UNSUPPORTED)))?;
    //     }
    // }

    // let open_ack_attachment = if ps_attachment.is_empty() {
    //     None
    // } else {
    //     let att =
    //         Attachment::try_from(&ps_attachment).map_err(|e| (e, Some(close::reason::INVALID)))?;
    //     Some(att)
    // }; @TODO

    let output = Output {
        cookie,
        initial_sn: open_syn.initial_sn,
        lease: open_syn.lease,
    };
    Ok(output)
}
