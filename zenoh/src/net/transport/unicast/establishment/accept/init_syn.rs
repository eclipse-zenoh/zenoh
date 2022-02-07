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
use super::super::AuthenticatedPeerLink;
use super::AResult;
use crate::net::link::LinkUnicast;
use crate::net::protocol::core::{SeqNumBytes, WhatAmI, ZenohId};
use crate::net::protocol::message::{CloseReason, InitSyn, WireProperties};
use crate::net::transport::TransportManager;
use zenoh_util::zerror;

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an InitSyn
pub(super) struct Output {
    pub(super) whatami: WhatAmI,
    pub(super) sn_bytes: SeqNumBytes,
    pub(super) zid: ZenohId,
    pub(super) is_qos: bool,
    pub(super) init_syn_auth_ext: WireProperties,
}
pub(super) async fn recv(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> AResult<Output> {
    // Wait to read an InitSyn
    let mut init_syn: InitSyn = link.recv().await.map_err(|e| (e, None))?;

    // Check the peer id associate to the authenticated link
    match auth_link.zid {
        Some(zid) => {
            if zid != init_syn.zid {
                let e = zerror!(
                    "Inconsistent PeerId in InitSyn on {}: {:?} {:?}",
                    link,
                    zid,
                    init_syn.zid
                );
                return Err((e.into(), Some(CloseReason::Invalid)));
            }
        }
        None => auth_link.zid = Some(init_syn.zid),
    }

    // Check if the version is supported
    if init_syn.version() != manager.config.version {
        let e = zerror!(
            "Rejecting InitSyn on {} because of unsupported Zenoh version from node: {}",
            link,
            init_syn.zid
        );
        return Err((e.into(), Some(CloseReason::Invalid)));
    }

    // Validate the InitSyn with the peer authenticators
    let init_syn_auth_ext: WireProperties = match init_syn.exts.authentication.take() {
        Some(auth) => auth.into_inner().into(),
        None => WireProperties::new(),
    };

    let output = Output {
        whatami: init_syn.whatami,
        sn_bytes: init_syn.sn_bytes,
        zid: init_syn.zid,
        is_qos: init_syn.exts.qos.is_some(),
        init_syn_auth_ext,
    };
    Ok(output)
}
