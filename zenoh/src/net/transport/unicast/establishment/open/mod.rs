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
    TransportInit, TransportUnicast,
};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::core::{PeerId, WhatAmI, ZInt};
use crate::net::protocol::proto::tmsg;
use crate::net::transport::{TransportManager, TransportPeer};
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::{zasynclock, zerror};

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
    link: &LinkUnicast,
    manager: &TransportManager,
    _auth_link: &AuthenticatedPeerLink,
    input: self::InputInit,
) -> OResult<TransportInit> {
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

    // Get inner transport
    let transport = ti
        .inner
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Add the link to the transport
    let _ = transport
        .add_link(link.clone())
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    log::debug!(
        "New transport link established with {}: {}",
        input.pid,
        link
    );

    Ok(ti)
}

// Finalize the transport, notify the callback and start the link tasks
async fn transport_finalize(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: self::OutputInit,
) -> ZResult<()> {
    // Retrive the transport's transport
    let t = input.transport.inner.get_inner()?;

    // Sync RX sequence number
    let _ = t.sync(input.initial_sn_rx).await;

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
                    .new_unicast(peer, input.transport.inner.clone())
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
    transport: TransportInit,
    lease: Duration,
    initial_sn_rx: ZInt,
}
async fn link_handshake(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &mut AuthenticatedPeerLink,
) -> OResult<OutputInit> {
    let output = init_syn::send(link, manager, auth_link).await?;
    let output = init_ack::recv(link, manager, auth_link, output).await?;

    let input = self::InputInit {
        pid: output.pid,
        whatami: output.whatami,
        sn_resolution: output.sn_resolution,
        is_shm: output.is_shm,
        is_qos: output.is_qos,
    };
    let transport = self::transport_init(link, manager, auth_link, input).await?;

    let initial_sn = transport
        .inner
        .get_inner()
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?
        .config
        .initial_sn_tx;
    let input = open_syn::Input {
        cookie: output.cookie,
        initial_sn,
        attachment: output.open_syn_attachment,
    };
    let output = open_syn::send(link, manager, auth_link, input).await?;
    let output = open_ack::recv(link, manager, auth_link, output).await?;

    let output = self::OutputInit {
        transport,
        lease: output.lease,
        initial_sn_rx: output.initial_sn,
    };
    Ok(output)
}

pub(crate) async fn open_link(
    manager: &TransportManager,
    link: &LinkUnicast,
) -> ZResult<TransportUnicast> {
    let mut auth_link = AuthenticatedPeerLink {
        src: link.get_src(),
        dst: link.get_src(),
        peer_id: None,
    };

    let res = link_handshake(link, manager, &mut auth_link).await;
    let output = match res {
        Ok(v) => v,
        Err((e, reason)) => {
            let _ = close_link(link, manager, &auth_link, reason).await;
            return Err(e);
        }
    };

    let transport = output.transport.inner.clone();
    let res = transport_finalize(link, manager, output).await;
    if let Err(e) = res {
        let _ = transport.close().await;
        return Err(e);
    }

    Ok(transport)
}
