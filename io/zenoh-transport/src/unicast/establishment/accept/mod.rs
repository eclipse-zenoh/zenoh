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
mod init_ack;
mod init_syn;
mod open_ack;
mod open_syn;

use crate::unicast::establishment::authenticator::AuthenticatedPeerLink;
use crate::unicast::establishment::{
    close_link, transport_finalize, transport_init, InputFinalize,
};
use crate::TransportManager;
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::transport::tmsg;
use zenoh_result::ZResult;

pub(super) type AError = (zenoh_result::Error, Option<u8>);
pub(super) type AResult<T> = Result<T, AError>;

pub(crate) async fn accept_link(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> ZResult<()> {
    // INIT handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    log::error!("{}", e);
                    close_link(link, manager, auth_link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    let output = step!(init_syn::recv(link, manager, auth_link).await);
    let output = step!(init_ack::send(link, manager, auth_link, output).await);
    let output = step!(open_syn::recv(link, manager, auth_link, output).await);

    // Initialize the transport
    let zid = output.cookie.zid;
    let input = super::InputInit {
        zid: output.cookie.zid,
        whatami: output.cookie.whatami,
        sn_resolution: output.cookie.sn_resolution,
        is_shm: output.is_shm,
        is_qos: output.cookie.is_qos,
    };
    let transport = step!(transport_init(manager, input)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))));

    // OPEN handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    match reason {
                        Some(tmsg::close_reason::MAX_LINKS) => log::debug!("{}", e),
                        _ => log::error!("{}", e),
                    }
                    if let Ok(ll) = transport.get_links() {
                        if ll.is_empty() {
                            let _ = manager.del_transport_unicast(&zid).await;
                        }
                    }
                    close_link(link, manager, auth_link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    // Add the link to the transport
    step!(step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .add_link(link.clone(), LinkUnicastDirection::Inbound)
    .map_err(|e| (e, Some(tmsg::close_reason::MAX_LINKS))));

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .sync(output.initial_sn)
    .await;

    log::debug!("New transport link established from {}: {}", zid, link);

    let initial_sn = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .config
    .initial_sn_tx;
    let input = open_ack::Input {
        initial_sn,
        attachment: output.open_ack_attachment,
    };
    let lease = output.lease;
    step!(open_ack::send(link, manager, auth_link, input).await);

    let input = InputFinalize {
        transport: transport.clone(),
        lease,
    };
    step!(transport_finalize(link, manager, input)
        .await
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))));

    Ok(())
}
