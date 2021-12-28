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
mod init_ack;
mod init_syn;
mod open_ack;
mod open_syn;

use super::authenticator::AuthenticatedPeerLink;
use super::{
    attachment_from_properties, close_link, properties_from_attachment, TransportConfigUnicast,
    TransportUnicast,
};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::core::{PeerId, WhatAmI, ZInt};
use crate::net::protocol::proto::tmsg;
use crate::net::transport::{TransportManager, TransportPeer};
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zasynclock;

type OError = (zenoh_util::core::Error, Option<u8>);
type OResult<T> = Result<T, OError>;

struct InputInit {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    is_shm: bool,
    is_qos: bool,
}
async fn transport_init(
    manager: &TransportManager,
    input: self::InputInit,
) -> OResult<TransportUnicast> {
    // Initialize the transport if it is new
    let initial_sn_tx = zasynclock!(manager.prng).gen_range(0..input.sn_resolution);

    let config = TransportConfigUnicast {
        peer: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        is_shm: input.is_shm,
        is_qos: input.is_qos,
        initial_sn_tx,
    };

    let ti = manager
        .init_transport_unicast(config)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    Ok(ti)
}

struct InputFinalize {
    transport: TransportUnicast,
    lease: Duration,
}
// Finalize the transport, notify the callback and start the link tasks
async fn transport_finalize(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: self::InputFinalize,
) -> ZResult<()> {
    // Retrive the transport's transport
    let transport = input.transport.get_inner()?;

    // Start the TX loop
    let _ = transport.start_tx(
        link,
        manager.config.unicast.keep_alive,
        manager.config.batch_size,
    )?;

    // Assign a callback if the transport is new
    match transport.get_callback() {
        Some(callback) => {
            // Notify the transport handler there is a new link on this transport
            callback.new_link(Link::from(link));
        }
        None => {
            let peer = TransportPeer {
                pid: transport.get_pid(),
                whatami: transport.get_whatami(),
                is_qos: transport.is_qos(),
                is_shm: transport.is_shm(),
                links: vec![Link::from(link)],
            };
            // Notify the transport handler that there is a new transport and get back a callback
            // NOTE: the read loop of the link the open message was sent on remains blocked
            //       until new_unicast() returns. The read_loop in the various links
            //       waits for any eventual transport to associate to.
            let callback = manager
                .config
                .handler
                .new_unicast(peer, input.transport.clone())?;
            // Set the callback on the transport
            transport.set_callback(callback);
        }
    }

    // Start the RX loop
    let _ = transport.start_rx(link, input.lease)?;

    Ok(())
}

pub(crate) async fn open_link(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> ZResult<TransportUnicast> {
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

    // INIT handshake
    let output = step!(init_syn::send(link, manager, auth_link).await);
    let output = step!(init_ack::recv(link, manager, auth_link, output).await);

    // Initialize the transport
    let pid = output.pid;
    let input = self::InputInit {
        pid,
        whatami: output.whatami,
        sn_resolution: output.sn_resolution,
        is_shm: output.is_shm,
        is_qos: output.is_qos,
    };
    let transport = step!(self::transport_init(manager, input).await);

    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    if let Ok(ll) = transport.get_links() {
                        if ll.is_empty() {
                            let _ = manager.del_transport_unicast(&pid).await;
                        }
                    }
                    close_link(link, manager, auth_link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    // OPEN handshake
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
    let _ = step!(step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .add_link(link.clone())
    .map_err(|e| (e, Some(tmsg::close_reason::MAX_LINKS))));

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID))))
    .sync(output.initial_sn)
    .await;

    log::debug!("New transport link established with {}: {}", pid, link);

    let output = self::InputFinalize {
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
