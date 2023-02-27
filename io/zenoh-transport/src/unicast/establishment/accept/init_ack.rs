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
use super::{init_syn, AResult};
use crate::{
    unicast::establishment::{Cookie, Zenoh080Cookie},
    TransportManager,
};
use rand::Rng;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_codec::{WCodec, Zenoh080};
use zenoh_core::zasynclock;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::{Field, Resolution, ZInt},
    transport::{
        close,
        init::{ext, InitAck},
        TransportMessage,
    },
};
use zenoh_result::zerror;

// Send an InitAck
pub(super) struct Output {
    pub(super) nonce: ZInt,
}

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: init_syn::Output,
) -> AResult<Output> {
    // Build the attachment from the authenticators
    // let mut ps_attachment = EstablishmentProperties::new();
    // let mut ps_cookie = EstablishmentProperties::new();
    // for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
    //     let (mut att, mut cke) = pa
    //         .handle_init_syn(
    //             auth_link,
    //             &cookie,
    //             input
    //                 .init_syn_properties
    //                 .remove(pa.id().into())
    //                 .map(|x| x.value),
    //         )
    //         .await
    //         .map_err(|e| (e, Some(close::reason::INVALID)))?;
    //     // Add attachment property if available
    //     if let Some(att) = att.take() {
    //         ps_attachment
    //             .insert(Property {
    //                 key: pa.id().into(),
    //                 value: att,
    //             })
    //             .map_err(|e| (e, Some(close::reason::UNSUPPORTED)))?;
    //     }
    //     // Add state in the cookie if avaiable
    //     if let Some(cke) = cke.take() {
    //         ps_cookie
    //             .insert(Property {
    //                 key: pa.id().into(),
    //                 value: cke,
    //             })
    //             .map_err(|e| (e, Some(close::reason::UNSUPPORTED)))?;
    //     }
    // }
    // cookie.properties = ps_cookie;

    // let attachment: Option<Attachment> = if ps_attachment.is_empty() {
    //     None
    // } else {
    //     let att =
    //         Attachment::try_from(&ps_attachment).map_err(|e| (e, Some(close::reason::INVALID)))?;
    //     Some(att)
    // }; @TODO

    // Compute the minimum SN resolution
    let resolution = {
        let mut res = Resolution::default();

        // Frame SN
        let i_fsn_res = input.resolution.get(Field::FrameSN);
        let m_fsn_res = manager.config.resolution.get(Field::FrameSN);
        res.set(Field::FrameSN, i_fsn_res.min(m_fsn_res));

        // Request ID
        let i_rid_res = input.resolution.get(Field::RequestID);
        let m_rid_res = manager.config.resolution.get(Field::RequestID);
        res.set(Field::RequestID, i_rid_res.min(m_rid_res));

        res
    };

    // Compute the minimum batch size
    let batch_size = input.batch_size.min(manager.config.batch_size);

    // Extensions: compute the QoS capabilities
    let is_qos = input.is_qos && manager.config.unicast.is_qos;
    let qos = is_qos.then_some(ext::QoS::new());

    // Create the cookie
    let nonce: ZInt = zasynclock!(manager.prng).gen();
    let cookie = Cookie {
        whatami: input.whatami,
        zid: input.zid,
        resolution,
        tx_batch_size: batch_size,
        nonce,
        is_qos,
        // properties: EstablishmentProperties::new(),
    };

    let mut encrypted = vec![];
    let mut writer = encrypted.writer();
    let mut codec = Zenoh080Cookie {
        prng: &mut *zasynclock!(manager.prng),
        cipher: &manager.cipher,
        codec: Zenoh080::new(),
    };
    codec.write(&mut writer, &cookie).map_err(|_| {
        (
            zerror!("Encoding cookie failed").into(),
            Some(close::reason::INVALID),
        )
    })?;

    // Send the cookie
    let cookie: ZSlice = encrypted.into();
    let message: TransportMessage = InitAck {
        version: manager.config.version,
        whatami: manager.config.whatami,
        zid: manager.config.zid,
        resolution,
        batch_size: manager.config.batch_size,
        cookie,
        qos,
        shm: None,  // @TODO
        auth: None, // @TODO
    }
    .into();

    // Send the message on the link
    let _ = link
        .write_transport_message(&message)
        .await
        .map_err(|e| (e, Some(close::reason::GENERIC)))?;

    let output = Output { nonce };
    Ok(output)
}
