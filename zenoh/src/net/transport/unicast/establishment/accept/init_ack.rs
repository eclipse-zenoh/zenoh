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
use super::{init_syn, AResult, Cookie};
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::ZInt;
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::message::extensions::{ZExt, ZExtPolicy};
use crate::net::protocol::message::{CloseReason, InitAck, WireProperties};
use crate::net::protocol::VERSION;
use crate::net::transport::unicast::establishment::authenticator::AuthenticatedPeerLink;
use crate::net::transport::TransportManager;
use rand::Rng;
use zenoh_util::crypto::hmac;
use zenoh_util::zasynclock;

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
    let agreed_sn_bytes = manager.config.sn_bytes.min(input.sn_bytes);

    // Build the fields for the InitAck message
    let whatami = manager.config.whatami;
    let azid = manager.config.zid;

    // Create the cookie
    let cookie = Cookie {
        whatami: input.whatami,
        zid: input.zid,
        sn_bytes: agreed_sn_bytes,
        is_qos: input.is_qos,
        nonce: zasynclock!(manager.prng).gen_range(0..ZInt::MAX),
    };

    // Build the attachment from the authenticators
    let mut init_ack_auth_ext = WireProperties::new();
    let mut init_ack_ckie_ext = WireProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let (mut ext, mut cke) = pa
            .handle_init_syn(
                auth_link,
                &cookie,
                input.init_syn_auth_ext.remove(&pa.id().into()),
            )
            .await
            .map_err(|e| (e, Some(CloseReason::Invalid)))?;
        // Add attachment property if available
        if let Some(ext) = ext.take() {
            init_ack_auth_ext.insert(pa.id().into(), ext);
        }
        // Add state in the cookie if avaiable
        if let Some(cke) = cke.take() {
            init_ack_ckie_ext.insert(pa.id().into(), cke);
        }
    }

    let encrypted = cookie
        .encrypt(
            &manager.cipher,
            &mut *zasynclock!(manager.prng),
            init_ack_ckie_ext,
        )
        .map_err(|e| (e, Some(CloseReason::Invalid)))?;

    // Compute the cookie hash
    let cookie_hash = hmac::digest(&encrypted);
    let cookie: ZSlice = encrypted.into();

    let mut message = InitAck::new(VERSION, whatami, azid, agreed_sn_bytes, cookie);
    if manager.config.unicast.is_qos {
        message.exts.qos = Some(ZExt::new((), ZExtPolicy::Ignore));
    }
    if !init_ack_auth_ext.is_empty() {
        message.exts.authentication = Some(ZExt::new(init_ack_auth_ext, ZExtPolicy::Ignore));
    }

    // Send the message on the link
    let _ = link.send(&mut message).await.map_err(|e| (e, None))?;

    let output = Output { cookie_hash };
    Ok(output)
}
