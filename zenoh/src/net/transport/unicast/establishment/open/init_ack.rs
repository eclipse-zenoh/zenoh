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
use super::super::authenticator::AuthenticatedPeerLink;
use super::{attachment_from_properties, properties_from_attachment, OResult};
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::{PeerId, Property, WhatAmI, ZInt};
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::proto::{tmsg, Attachment, Close, TransportBody};
use crate::net::transport::unicast::establishment::authenticator::PeerAuthenticatorId;
use crate::net::transport::unicast::establishment::EstablishmentProperties;
use crate::net::transport::TransportManager;
use zenoh_util::zerror;

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output {
    pub(super) pid: PeerId,
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
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
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
            return Err((
                zerror!(
                    "Received a close message (reason {}) in response to an InitSyn on: {}",
                    reason,
                    link,
                )
                .into(),
                None,
            ));
        }
        _ => {
            return Err((
                zerror!(
                    "Received an invalid message in response to an InitSyn on {}: {:?}",
                    link,
                    msg.body
                )
                .into(),
                Some(tmsg::close_reason::INVALID),
            ));
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
    auth_link.peer_id = Some(init_ack.pid);

    let mut init_ack_properties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };

    let mut is_shm = false;
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let mut att = pa
            .handle_init_ack(
                auth_link,
                &init_ack.pid,
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
                    if e.is::<zenoh_util::core::zresult::ShmError>() {
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
                .map_err(|e| (e, None))?;
        }
    }

    let output = Output {
        pid: init_ack.pid,
        whatami: init_ack.whatami,
        sn_resolution,
        is_qos: init_ack.is_qos,
        is_shm,
        cookie: init_ack.cookie,
        open_syn_attachment: attachment_from_properties(&ps_attachment).ok(),
    };
    Ok(output)
}

// // Check if a transport is already open with the target peer
// let mut guard = zasynclock!(manager.state.unicast.opened);
// let (sn_resolution, initial_sn_tx, is_opened) = if let Some(s) = guard.get(&init_ack.pid) {
//     if let Some(sn_resolution) = init_ack.sn_resolution {
//         if sn_resolution != s.sn_resolution {
//             return Err((
//                 zerror!(
//                     "Rejecting InitAck on {}. Invalid sn resolution: {}",
//                     link,
//                     sn_resolution
//                 )
//                 .into(),
//                 Some(tmsg::close_reason::INVALID),
//             ));
//         }
//     }
//     (s.sn_resolution, s.initial_sn, true)
// } else {
//     let sn_resolution = match init_ack.sn_resolution {
//         Some(sn_resolution) => {
//             if sn_resolution > manager.config.sn_resolution {
//                 return Err((
//                     zerror!(
//                         "Rejecting InitAck on {}. Invalid sn resolution: {}",
//                         link,
//                         sn_resolution
//                     )
//                     .into(),
//                     Some(tmsg::close_reason::INVALID),
//                 ));
//             }
//             sn_resolution
//         }
//         None => manager.config.sn_resolution,
//     };
//     let initial_sn_tx = zasynclock!(manager.prng).gen_range(0..sn_resolution);
//     (sn_resolution, initial_sn_tx, false)
// };
