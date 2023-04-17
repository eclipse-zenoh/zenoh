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
use crate::unicast::establishment::open::OResult;
use crate::unicast::establishment::{
    authenticator::AuthenticatedPeerLink, EstablishmentProperties,
};
use crate::TransportManager;
use std::convert::TryFrom;
use zenoh_buffers::ZSlice;
use zenoh_core::zasyncread;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    common::Attachment,
    core::{Property, WhatAmI, ZInt, ZenohId},
    transport::{tmsg, Close, TransportBody},
};
use zenoh_result::zerror;

#[cfg(feature = "shared-memory")]
use crate::unicast::establishment::authenticator::PeerAuthenticatorId;

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output {
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) is_qos: bool,
    pub(super) is_shm: bool,
    pub(super) cookie: ZSlice,
    pub(super) open_syn_attachment: Option<Attachment>,
}
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
    _input: super::init_syn::Output,
) -> OResult<Output> {
    // Wait to read an InitAck
    let mut messages = link
        .read_transport_message()
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages in response to an InitSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let init_ack = match msg.body {
        TransportBody::InitAck(init_ack) => init_ack,
        TransportBody::Close(Close { reason, .. }) => {
            let e = zerror!(
                "Received a close message (reason {}) in response to an InitSyn on: {}",
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
                "Received an invalid message in response to an InitSyn on {}: {:?}",
                link,
                msg.body
            );
            log::error!("{}", e);
            return Err((e.into(), Some(tmsg::close_reason::INVALID)));
        }
    };

    let sn_resolution = match init_ack.sn_resolution {
        Some(sn_resolution) => {
            if sn_resolution > manager.config.sn_resolution {
                return Err((
                    zerror!(
                        "Rejecting InitAck on {}. Invalid sn resolution: {}",
                        link,
                        sn_resolution
                    )
                    .into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
            sn_resolution
        }
        None => manager.config.sn_resolution,
    };

    // Store the peer id associate do this link
    auth_link.peer_id = Some(init_ack.zid);

    let mut init_ack_properties = match msg.attachment.take() {
        Some(att) => EstablishmentProperties::try_from(&att)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?,
        None => EstablishmentProperties::new(),
    };

    #[allow(unused_mut)]
    let mut is_shm = false;
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        #[allow(unused_mut)]
        let mut att = pa
            .handle_init_ack(
                auth_link,
                &init_ack.zid,
                sn_resolution,
                init_ack_properties.remove(pa.id().into()).map(|x| x.value),
            )
            .await;

        #[cfg(feature = "shared-memory")]
        if pa.id() == PeerAuthenticatorId::Shm {
            // Check if SHM has been validated from the other side
            att = match att {
                Ok(att) => {
                    is_shm = att.is_some();
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

    let open_syn_attachment = if ps_attachment.is_empty() {
        None
    } else {
        let att = Attachment::try_from(&ps_attachment)
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        Some(att)
    };

    let output = Output {
        zid: init_ack.zid,
        whatami: init_ack.whatami,
        sn_resolution,
        is_qos: init_ack.is_qos,
        is_shm,
        cookie: init_ack.cookie,
        open_syn_attachment,
    };
    Ok(output)
}
