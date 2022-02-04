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
use crate::net::protocol::message::extensions::{ZExt, ZExtPolicy};
use crate::net::protocol::message::{InitSyn, WireProperties};
use crate::net::protocol::VERSION;
use crate::net::transport::unicast::establishment::authenticator::AuthenticatedPeerLink;
use crate::net::transport::TransportManager;

/*************************************/
/*              OPEN                 */
/*************************************/
pub(super) struct Output;

pub(super) async fn send(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> OResult<Output> {
    let mut init_syn_auth_ext = WireProperties::new();
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        let mut ext = pa
            .get_init_syn_properties(auth_link, &manager.config.zid)
            .await
            .map_err(|e| (e, None))?;
        if let Some(ext) = ext.take() {
            init_syn_auth_ext.insert(pa.id().into(), ext);
        }
    }

    // Build and send the InitSyn message
    let mut message = InitSyn::new(
        VERSION,
        manager.config.whatami,
        manager.config.zid,
        manager.config.sn_bytes,
    );
    if manager.config.unicast.is_qos {
        message.exts.qos = Some(ZExt::new((), ZExtPolicy::Ignore));
    }
    if !init_syn_auth_ext.is_empty() {
        message.exts.authentication = Some(ZExt::new(init_syn_auth_ext, ZExtPolicy::Ignore));
    }

    link.send(&mut message).await.map_err(|e| (e, None))?;

    let output = Output;
    Ok(output)
}
