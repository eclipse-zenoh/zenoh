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
use crate::unicast::establishment::open::OResult;
use crate::TransportManager;
use zenoh_buffers::ZSlice;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::{Field, Resolution, WhatAmI, ZenohId},
    transport::{
        close::{self, Close},
        TransportBody,
    },
};
use zenoh_result::zerror;

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output {
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) resolution: Resolution,
    pub(super) batch_size: u16,
    pub(super) cookie: ZSlice,
    // Extensions
    pub(super) is_qos: bool,
}

pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: super::init_syn::Output,
) -> OResult<Output> {
    // Wait to read an InitAck
    let mut messages = link
        .read_transport_message()
        .await
        .map_err(|e| (e, Some(close::reason::INVALID)))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages in response to an InitSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(close::reason::INVALID),
        ));
    }

    let msg = messages.remove(0);
    let init_ack = match msg.body {
        TransportBody::InitAck(init_ack) => init_ack,
        TransportBody::Close(Close { reason, .. }) => {
            let e = zerror!(
                "Received a close message (reason {}) in response to an InitSyn on: {}",
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
                "Received an invalid message in response to an InitSyn on {}: {:?}",
                link,
                msg.body
            );
            log::error!("{}", e);
            return Err((e.into(), Some(close::reason::INVALID)));
        }
    };

    // Compute the minimum SN resolution
    let resolution = {
        let i_fsn_res = init_ack.resolution.get(Field::FrameSN);
        let m_fsn_res = manager.config.resolution.get(Field::FrameSN);

        if i_fsn_res > m_fsn_res {
            let e = zerror!("Invalid SN resolution on {}: {:?}", link, i_fsn_res);
            log::error!("{}", e);
            return Err((e.into(), Some(close::reason::INVALID)));
        }

        let mut res = Resolution::default();
        res.set(Field::FrameSN, i_fsn_res);
        res
    };

    // Compute the minimum batch size
    let batch_size = manager.config.batch_size.min(init_ack.batch_size);

    // Compute QoS
    let is_qos = input.is_qos && init_ack.qos.is_some();

    // // Store the peer id associate do this link
    // auth_link.peer_id = Some(init_ack.zid);

    // let mut init_ack_properties = match msg.attachment.take() {
    //     Some(att) => EstablishmentProperties::try_from(&att)
    //         .map_err(|e| (e, Some(close::reason::INVALID)))?,
    //     None => EstablishmentProperties::new(),
    // };

    // #[allow(unused_mut)]
    // let mut is_shm = false;
    // let mut ps_attachment = EstablishmentProperties::new();
    // for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
    //     #[allow(unused_mut)]
    //     let mut att = pa
    //         .handle_init_ack(
    //             auth_link,
    //             &init_ack.zid,
    //             sn_resolution,
    //             init_ack_properties.remove(pa.id().into()).map(|x| x.value),
    //         )
    //         .await;

    //     #[cfg(feature = "shared-memory")]
    //     if pa.id() == PeerAuthenticatorId::Shm {
    //         // Check if SHM has been validated from the other side
    //         att = match att {
    //             Ok(att) => {
    //                 is_shm = att.is_some();
    //                 Ok(att)
    //             }
    //             Err(e) => {
    //                 if e.is::<zenoh_result::ShmError>() {
    //                     is_shm = false;
    //                     Ok(None)
    //                 } else {
    //                     Err(e)
    //                 }
    //             }
    //         };
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

    // let open_syn_attachment = if ps_attachment.is_empty() {
    //     None
    // } else {
    //     let att =
    //         Attachment::try_from(&ps_attachment).map_err(|e| (e, Some(close::reason::INVALID)))?;
    //     Some(att)
    // }; @TODO

    let output = Output {
        zid: init_ack.zid,
        whatami: init_ack.whatami,
        resolution,
        batch_size,
        cookie: init_ack.cookie,
        is_qos,
    };

    Ok(output)
}
