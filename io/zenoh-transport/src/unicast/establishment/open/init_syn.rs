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
use super::OResult;
use crate::TransportManager;
use zenoh_link::LinkUnicast;
use zenoh_protocol::transport::{
    close,
    init::{ext, InitSyn},
    TransportMessage,
};

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output {
    pub(super) is_qos: bool,
}

pub(super) async fn send(link: &LinkUnicast, manager: &TransportManager) -> OResult<Output> {
    // let mut ps_attachment = EstablishmentProperties::new();
    // for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
    //     let mut att = pa
    //         .get_init_syn_properties(auth_link, &manager.config.zid)
    //         .await
    //         .map_err(|e| (e, Some(close::reason::UNSUPPORTED)))?;
    //     if let Some(att) = att.take() {
    //         ps_attachment
    //             .insert(Property {
    //                 key: pa.id().into(),
    //                 value: att,
    //             })
    //             .map_err(|e| (e, Some(close::reason::UNSUPPORTED)))?;
    //     }
    // }

    // Build QoS extension
    let qos = manager.config.unicast.is_qos.then_some(ext::QoS::new());

    let msg: TransportMessage = InitSyn {
        version: manager.config.version,
        whatami: manager.config.whatami,
        zid: manager.config.zid,
        batch_size: manager.config.batch_size,
        resolution: manager.config.resolution,
        qos,
        shm: None,  // @TODO
        auth: None, // @TODO
    }
    .into();

    let _ = link
        .write_transport_message(&msg)
        .await
        .map_err(|e| (e, Some(close::reason::GENERIC)))?;

    let output = Output {
        is_qos: qos.is_some(),
    };
    Ok(output)
}
