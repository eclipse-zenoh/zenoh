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
    attachment_from_properties, close_link, properties_from_attachment, Cookie,
    EstablishmentProperties, TransportUnicast,
};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::core::ZInt;
use crate::net::protocol::proto::tmsg;
use crate::net::transport::{TransportConfigUnicast, TransportManager, TransportPeer};
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zerror;

pub(super) type AError = (zenoh_util::core::Error, Option<u8>);
pub(super) type AResult<T> = Result<T, AError>;

/*************************************/
/*             ACCEPT                */
/*************************************/
struct InputInit {
    cookie: Cookie,
    initial_sn: ZInt,
    is_shm: bool,
}
async fn transport_init(
    link: &LinkUnicast,
    manager: &TransportManager,
    _auth_link: &AuthenticatedPeerLink,
    input: self::InputInit,
) -> AResult<TransportUnicast> {
    // Initialize the transport if it is new
    let open_ack_initial_sn = zasynclock!(manager.prng).gen_range(0..input.cookie.sn_resolution);

    let config = TransportConfigUnicast {
        peer: input.cookie.pid,
        whatami: input.cookie.whatami,
        sn_resolution: input.cookie.sn_resolution,
        is_shm: input.is_shm,
        is_qos: input.cookie.is_qos,
        initial_sn_tx: open_ack_initial_sn,
        initial_sn_rx: input.initial_sn,
    };
    let transport = manager
        .init_transport_unicast(config)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Retrieve the inner transport
    let t = transport.get_inner().map_err(|e| (e, None))?;

    // Add the link to the transport
    let _ = t
        .add_link(link.clone())
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    log::debug!(
        "New transport link established from {}: {}",
        input.cookie.pid,
        link
    );

    Ok(transport)
}

// Finalize the transport, notify the callback and start the link tasks
async fn transport_finalize(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: self::OutputInit,
) -> ZResult<()> {
    // Retrive the transport's transport
    let t = input.transport.get_inner()?;

    // Acquire the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = t.get_alive().await;
    if *a_guard {
        // Start the TX loop
        let _ = t.start_tx(
            link,
            manager.config.unicast.keep_alive,
            manager.config.batch_size,
        )?;

        // Assign a callback if the transport is new
        match t.get_callback() {
            Some(callback) => {
                // Notify the transport handler there is a new link on this transport
                callback.new_link(Link::from(link));
            }
            None => {
                let peer = TransportPeer {
                    pid: t.get_pid(),
                    whatami: t.get_whatami(),
                    is_qos: t.is_qos(),
                    is_shm: t.is_shm(),
                    links: vec![Link::from(link)],
                };
                // Notify the transport handler that there is a new transport and get back a callback
                // NOTE: the read loop of the link the open message was sent on remains blocked
                //       until the new_transport() returns. The read_loop in the various links
                //       waits for any eventual transport to associate to.
                let callback = manager
                    .config
                    .handler
                    .new_unicast(peer, input.transport.clone())
                    .map_err(|e| {
                        zerror!(
                            "Rejecting OpenSyn on: {}. New transport error: {:?}",
                            link,
                            e
                        )
                    })?;
                // Set the callback on the transport
                t.set_callback(callback);
            }
        }

        // Start the RX loop
        let _ = t.start_rx(link, input.lease)?;
    }
    drop(a_guard);

    Ok(())
}

struct OutputInit {
    transport: TransportUnicast,
    lease: Duration,
}
async fn link_handshake(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> AResult<OutputInit> {
    let output = init_syn::recv(link, manager, auth_link).await?;
    let output = init_ack::send(link, manager, auth_link, output).await?;
    let output = open_syn::recv(link, manager, auth_link, output).await?;

    let input = self::InputInit {
        cookie: output.cookie,
        initial_sn: output.initial_sn,
        is_shm: output.is_shm,
    };
    let transport = self::transport_init(link, manager, auth_link, input).await?;

    let initial_sn = transport
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?
        .config
        .initial_sn_tx;
    let input = open_ack::Input {
        initial_sn,
        attachment: output.open_ack_attachment,
    };
    let _ = open_ack::send(link, manager, auth_link, input).await?;

    let output = self::OutputInit {
        transport,
        lease: output.lease,
    };
    Ok(output)
}

pub(crate) async fn accept_link(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> ZResult<()> {
    let res = link_handshake(link, manager, auth_link).await;
    let output = match res {
        Ok(output) => output,
        Err((e, reason)) => {
            close_link(manager, link, auth_link, reason).await;
            return Err(e);
        }
    };

    let transport = output.transport.clone();
    let res = transport_finalize(link, manager, output).await;
    if let Err(e) = res {
        let _ = transport.close().await;
        return Err(e);
    }

    Ok(())
}
