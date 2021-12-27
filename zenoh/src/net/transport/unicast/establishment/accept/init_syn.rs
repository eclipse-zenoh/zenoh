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
use super::super::{properties_from_attachment, AuthenticatedPeerLink, EstablishmentProperties};
use super::AResult;
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::{PeerId, WhatAmI, ZInt};
use crate::net::protocol::proto::{tmsg, TransportBody};
use crate::net::transport::TransportManager;
use zenoh_util::zerror;

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an InitSyn
pub(super) struct Output {
    pub(super) whatami: WhatAmI,
    pub(super) pid: PeerId,
    pub(super) sn_resolution: ZInt,
    pub(super) is_qos: bool,
    pub(super) init_syn_properties: EstablishmentProperties,
}
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> AResult<Output> {
    // Wait to read an InitSyn
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages instead of a single InitSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let init_syn = match msg.body {
        TransportBody::InitSyn(init_syn) => init_syn,
        _ => {
            return Err((
                zerror!(
                    "Received invalid message instead of an InitSyn on {}: {:?}",
                    link,
                    msg.body
                )
                .into(),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };

    // Check the peer id associate to the authenticated link
    match auth_link.peer_id {
        Some(pid) => {
            if pid != init_syn.pid {
                return Err((
                    zerror!(
                        "Inconsistent PeerId in InitSyn on {}: {:?} {:?}",
                        link,
                        pid,
                        init_syn.pid
                    )
                    .into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        }
        None => auth_link.peer_id = Some(init_syn.pid),
    }

    // Check if the version is supported
    if init_syn.version != manager.config.version {
        return Err((
            zerror!(
                "Rejecting InitSyn on {} because of unsupported Zenoh version from peer: {}",
                link,
                init_syn.pid
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    // Validate the InitSyn with the peer authenticators
    let init_syn_properties: EstablishmentProperties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };

    let output = Output {
        whatami: init_syn.whatami,
        pid: init_syn.pid,
        sn_resolution: init_syn.sn_resolution,
        is_qos: init_syn.is_qos,
        init_syn_properties,
    };
    Ok(output)
}
