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

use super::authenticator::AuthenticatedPeerLink;
use crate::unicast::establishment::{close_link, transport_finalize, InputFinalize, InputInit};
use crate::{TransportManager, TransportUnicast};
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::transport::tmsg;
use zenoh_result::ZResult;

type OError = (zenoh_result::Error, Option<u8>);
type OResult<T> = Result<T, OError>;

pub(crate) async fn open_link(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> ZResult<TransportUnicast> {
    // INIT handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    close_link(link, manager, auth_link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    let output = step!(init_syn::send(link, manager, auth_link).await);
    let output = step!(init_ack::recv(link, manager, auth_link, output).await);

    // Initialize the transport
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err(e) => {
                    close_link(link, manager, auth_link, Some(tmsg::close_reason::INVALID)).await;
                    return Err(e);
                }
            }
        };
    }

    let zid = output.zid;
    let input = InputInit {
        zid,
        whatami: output.whatami,
        sn_resolution: output.sn_resolution,
        is_shm: output.is_shm,
        is_qos: output.is_qos,
    };
    let transport = step!(super::transport_init(manager, input).await);

    // OPEN handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
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

    let initial_sn = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .config
    .initial_sn_tx;
    let input = open_syn::Input {
        cookie: output.cookie,
        initial_sn,
        attachment: output.open_syn_attachment,
    };
    let output = step!(open_syn::send(link, manager, auth_link, input).await);
    let output = step!(open_ack::recv(link, manager, auth_link, output).await);

    // Finalize the transport
    // Add the link to the transport
    step!(step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .add_link(link.clone(), LinkUnicastDirection::Outbound)
    .map_err(|e| (e, Some(tmsg::close_reason::MAX_LINKS))));

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .sync(output.initial_sn)
    .await;

    log::debug!("New transport link established with {}: {}", zid, link);

    let output = InputFinalize {
        transport,
        lease: output.lease,
    };
    let transport = output.transport.clone();
    let res = transport_finalize(link, manager, output).await;
    if let Err(e) = res {
        let _ = transport.close().await;
        return Err(e);
    }

    Ok(transport)
}
