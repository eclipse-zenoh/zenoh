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
use super::super::authenticator::{AuthenticatedPeerLink, PeerAuthenticatorId};
use super::AResult;
use super::Cookie;
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::ZInt;
use crate::net::protocol::message::{CloseReason, OpenSyn, WireProperties};
use crate::net::transport::TransportManager;
use std::time::Duration;
use zenoh_util::crypto::hmac;
use zenoh_util::zerror;

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an OpenSyn
pub(super) struct Output {
    pub(super) cookie: Cookie,
    pub(super) initial_sn: ZInt,
    pub(super) lease: Duration,
    pub(super) is_shm: bool,
    pub(super) open_ack_auth_ext: WireProperties,
}
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &AuthenticatedPeerLink,
    input: super::init_ack::Output,
) -> AResult<Output> {
    // Wait to read an OpenSyn
    let mut open_syn: OpenSyn = link.recv().await.map_err(|e| (e, None))?;

    // Verify that the cookie is the one we sent
    let encrypted = open_syn.cookie.to_vec();
    if input.cookie_hash != hmac::digest(&encrypted) {
        let e = zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link);
        return Err((e.into(), Some(CloseReason::Invalid)));
    }

    // Decrypt the cookie with the cyper
    let (cookie, mut ps_cookie) =
        Cookie::decrypt(encrypted, &manager.cipher).map_err(|e| (e, Some(CloseReason::Invalid)))?;

    // Validate with the peer authenticators
    let mut open_syn_auth: WireProperties = open_syn
        .exts
        .authentication
        .take()
        .map_or_else(WireProperties::new, |v| v.into_inner().into());

    let mut is_shm = false;
    let mut open_ack_auth_ext = WireProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let mut ext = pa
            .handle_open_syn(
                auth_link,
                &cookie,
                (
                    open_syn_auth.remove(&pa.id().into()),
                    ps_cookie.remove(&pa.id().into()),
                ),
            )
            .await;

        #[cfg(feature = "shared-memory")]
        if pa.id() == PeerAuthenticatorId::Shm {
            // Check if SHM has been validated from the other side
            ext = match ext {
                Ok(att) => {
                    is_shm = true;
                    Ok(att)
                }
                Err(e) => {
                    if e.is::<zenoh_util::core::zresult::ShmError>() {
                        is_shm = false;
                        Ok(None)
                    } else {
                        Err(e)
                    }
                }
            };
        }

        let mut ext = ext.map_err(|e| (e, Some(CloseReason::Invalid)))?;
        if let Some(ext) = ext.take() {
            open_ack_auth_ext.insert(pa.id().into(), ext);
        }
    }

    let output = Output {
        cookie,
        initial_sn: open_syn.initial_sn,
        lease: open_syn.lease,
        is_shm,
        open_ack_auth_ext,
    };
    Ok(output)
}
