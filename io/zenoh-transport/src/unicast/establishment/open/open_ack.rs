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
use crate::{unicast::establishment::open::OResult, TransportManager};
use std::time::Duration;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::{Field, ZInt},
    transport::{
        close::{self, Close},
        TransportBody,
    },
};
use zenoh_result::zerror;

pub(super) struct Output {
    pub(super) initial_sn: ZInt,
    pub(super) lease: Duration,
}

pub(super) async fn recv(
    link: &LinkUnicast,
    _manager: &TransportManager,
    input: super::open_syn::Output,
) -> OResult<Output> {
    // Wait to read an OpenAck
    let mut messages = link
        .read_transport_message()
        .await
        .map_err(|e| (e, Some(close::reason::INVALID)))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages in response to an OpenSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(close::reason::INVALID),
        ));
    }

    let msg = messages.remove(0);
    let open_ack = match msg.body {
        TransportBody::OpenAck(open_ack) => open_ack,
        TransportBody::Close(Close { reason, .. }) => {
            let e = zerror!(
                "Received a close message (reason {}) in response to an OpenSyn on: {:?}",
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
                "Received an invalid message in response to an OpenSyn on {}: {:?}",
                link,
                msg.body
            );
            log::error!("{}", e);
            return Err((e.into(), Some(close::reason::INVALID)));
        }
    };

    // Verify that initial_sn is within the agreed resolution
    if open_ack.initial_sn > input.resolution.get(Field::FrameSN).mask() {
        let e = zerror!(
            "Received invalid initial SN {}: {:?}",
            link,
            open_ack.initial_sn
        );
        log::error!("{}", e);
        return Err((e.into(), Some(close::reason::INVALID)));
    }

    // let mut opean_ack_properties = match msg.attachment.take() {
    //     Some(att) => EstablishmentProperties::try_from(&att)
    //         .map_err(|e| (e, Some(close::reason::INVALID)))?,
    //     None => EstablishmentProperties::new(),
    // };
    // for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
    //     let _ = pa
    //         .handle_open_ack(
    //             auth_link,
    //             opean_ack_properties.remove(pa.id().into()).map(|x| x.value),
    //         )
    //         .await
    //         .map_err(|e| (e, Some(close::reason::INVALID)))?;
    // } @TODO

    let output = Output {
        initial_sn: open_ack.initial_sn,
        lease: open_ack.lease,
    };
    Ok(output)
}
