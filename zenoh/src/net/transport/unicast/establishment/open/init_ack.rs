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
use super::OResult;
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::{SeqNumBytes, WhatAmI, ZenohId};
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::message::{CloseReason, InitAck, WireProperties};
use crate::net::transport::unicast::establishment::authenticator::AuthenticatedPeerLink;
use crate::net::transport::unicast::establishment::authenticator::PeerAuthenticatorId;
use crate::net::transport::TransportManager;
use zenoh_util::zerror;

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output {
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) sn_bytes: SeqNumBytes,
    pub(super) is_qos: bool,
    pub(super) is_shm: bool,
    pub(super) cookie: ZSlice,
    pub(super) open_syn_auth_ext: WireProperties,
}
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
    _input: super::init_syn::Output,
) -> OResult<Output> {
    // Wait to read an InitAck
    let mut init_ack: InitAck = link.recv().await.map_err(|e| (e, None))?;

    if init_ack.sn_bytes > manager.config.sn_bytes {
        return Err((
            zerror!(
                "Rejecting InitAck on {}. Invalid sn resolution: {}",
                link,
                init_ack.sn_bytes.value()
            )
            .into(),
            Some(CloseReason::Invalid),
        ));
    }

    // Store the zenoh id associated to this link
    auth_link.zid = Some(init_ack.zid);

    let mut init_ack_auth_ext: WireProperties = match init_ack.exts.authentication.take() {
        Some(auth) => auth.into_inner().into(),
        None => WireProperties::new(),
    };

    let mut is_shm = false;
    let mut open_syn_auth_ext = WireProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let mut ext = pa
            .handle_init_ack(
                auth_link,
                &init_ack.zid,
                init_ack.sn_bytes,
                init_ack_auth_ext.remove(&pa.id().into()),
            )
            .await;

        #[cfg(feature = "shared-memory")]
        if pa.id() == PeerAuthenticatorId::Shm {
            // Check if SHM has been validated from the other side
            ext = match ext {
                Ok(shm) => {
                    is_shm = shm.is_some();
                    Ok(shm)
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
            open_syn_auth_ext.insert(pa.id().into(), ext);
        }
    }

    let output = Output {
        zid: init_ack.zid,
        whatami: init_ack.whatami,
        sn_bytes: init_ack.sn_bytes,
        is_qos: init_ack.exts.qos.is_some(),
        is_shm,
        cookie: init_ack.cookie,
        open_syn_auth_ext,
    };
    Ok(output)
}
