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
use super::authenticator::{AuthenticatedPeerLink, PeerAuthenticatorId};
use super::{attachment_from_properties, close_link, properties_from_attachment};
use super::{Cookie, EstablishmentProperties};
use super::{TransportConfigUnicast, TransportUnicast};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::core::{PeerId, Property, WhatAmI, ZInt};
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::proto::{
    tmsg, Attachment, Close, OpenSyn, TransportBody, TransportMessage,
};
use crate::net::transport::unicast::manager::Opened;
use crate::net::transport::{TransportManager, TransportPeer};
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::crypto::hmac;
use zenoh_util::{zasynclock, zerror};

type IError = (zenoh_util::core::Error, Option<u8>);
type IResult<T> = Result<T, IError>;

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an InitSyn
struct AcceptInitSynOutput {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    is_qos: bool,
    init_syn_properties: EstablishmentProperties,
}
async fn accept_recv_init_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> IResult<AcceptInitSynOutput> {
    // Wait to read an InitSyn
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages instead of a single InitSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let init_syn = match msg.body {
        TransportBody::InitSyn(init_syn) => init_syn,
        _ => {
            return Err((
                zerror!(
                    "Received invalid message instead of an InitSyn on {}: {:?}",
                    link,
                    msg.body
                )
                .into(),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };

    // Store the peer id associate to this link
    match auth_link.peer_id {
        Some(pid) => {
            if pid != init_syn.pid {
                return Err((
                    zerror!(
                        "Inconsistent PeerId in InitSyn on {}: {:?} {:?}",
                        link,
                        pid,
                        init_syn.pid
                    )
                    .into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        }
        None => auth_link.peer_id = Some(init_syn.pid),
    }

    // Check if we are allowed to open more links if the transport is established
    if let Some(t) = manager.get_transport_unicast(&init_syn.pid) {
        let t = t
            .get_transport()
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

        // Check if we have reached maximum number of links for this transport
        if !t.can_add_link(link) {
            return Err((
                zerror!(
                    "Rejecting InitSyn on {} because of max num links for peer: {}",
                    link,
                    init_syn.pid
                )
                .into(),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    }

    // Check if the version is supported
    if init_syn.version != manager.config.version {
        return Err((
            zerror!(
                "Rejecting InitSyn on {} because of unsupported Zenoh version from peer: {}",
                link,
                init_syn.pid
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    // Validate the InitSyn with the peer authenticators
    let init_syn_properties: EstablishmentProperties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };

    let output = AcceptInitSynOutput {
        whatami: init_syn.whatami,
        pid: init_syn.pid,
        sn_resolution: init_syn.sn_resolution,
        is_qos: init_syn.is_qos,
        init_syn_properties,
    };
    Ok(output)
}

// Send an InitAck
struct AcceptInitAckOutput {}
async fn accept_send_init_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    mut input: AcceptInitSynOutput,
) -> IResult<AcceptInitAckOutput> {
    // Compute the minimum SN Resolution
    let agreed_sn_resolution = manager.config.sn_resolution.min(input.sn_resolution);

    // Build the fields for the InitAck message
    let whatami = manager.config.whatami;
    let apid = manager.config.pid;
    let sn_resolution = if agreed_sn_resolution == input.sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };

    // Create the cookie
    let cookie = Cookie {
        whatami: input.whatami,
        pid: input.pid,
        sn_resolution: agreed_sn_resolution,
        is_qos: input.is_qos,
        nonce: zasynclock!(manager.prng).gen_range(0..agreed_sn_resolution),
    };

    // Build the attachment
    let mut ps_attachment = EstablishmentProperties::new();
    let mut ps_cookie = EstablishmentProperties::new();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let (mut att, mut cke) = pa
            .handle_init_syn(
                auth_link,
                &cookie,
                input
                    .init_syn_properties
                    .remove(pa.id().into())
                    .map(|x| x.value),
            )
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        if let Some(att) = att.take() {
            ps_attachment
                .insert(Property {
                    key: pa.id().into(),
                    value: att,
                })
                .map_err(|e| (e, None))?;
        }
        if let Some(cke) = cke.take() {
            ps_cookie
                .insert(Property {
                    key: pa.id().into(),
                    value: cke,
                })
                .map_err(|e| (e, None))?;
        }
    }
    let attachment = attachment_from_properties(&ps_attachment).ok();

    let encrypted = cookie
        .encrypt(&manager.cipher, &mut *zasynclock!(manager.prng), ps_cookie)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Compute and store cookie hash
    let hash = hmac::digest(&encrypted);
    zasynclock!(manager.state.unicast.incoming).insert(link.clone(), Some(hash));

    // Send the cookie
    let cookie: ZSlice = encrypted.into();
    let mut message = TransportMessage::make_init_ack(
        whatami,
        apid,
        sn_resolution,
        input.is_qos,
        cookie,
        attachment,
    );

    // Send the message on the link
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, None))?;

    let output = AcceptInitAckOutput {};
    Ok(output)
}

// Read and eventually accept an OpenSyn
struct AcceptOpenSynOutput {
    cookie: Cookie,
    initial_sn: ZInt,
    lease: Duration,
    is_shm: bool,
    open_ack_attachment: Option<Attachment>,
}
async fn accept_recv_open_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    _input: AcceptInitAckOutput,
) -> IResult<AcceptOpenSynOutput> {
    // Wait to read an OpenSyn
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        return Err((
            zerror!(
                "Received multiple messages instead of a single OpenSyn on {}: {:?}",
                link,
                messages,
            )
            .into(),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (open_syn_lease, open_syn_initial_sn, open_syn_cookie) = match msg.body {
        TransportBody::OpenSyn(OpenSyn {
            lease,
            initial_sn,
            cookie,
        }) => (lease, initial_sn, cookie),
        TransportBody::Close(Close { reason, .. }) => {
            return Err((
                zerror!(
                    "Received a close message (reason {}) instead of an OpenSyn on: {:?}",
                    reason,
                    link,
                )
                .into(),
                None,
            ));
        }
        _ => {
            return Err((
                zerror!(
                    "Received invalid message instead of an OpenSyn on {}: {:?}",
                    link,
                    msg.body
                )
                .into(),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };
    let encrypted = open_syn_cookie.to_vec();

    // Verify that the cookie is the one we sent
    match zasynclock!(manager.state.unicast.incoming).get(link) {
        Some(cookie_hash) => match cookie_hash {
            Some(cookie_hash) => {
                if cookie_hash != &hmac::digest(&encrypted) {
                    return Err((
                        zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link).into(),
                        Some(tmsg::close_reason::INVALID),
                    ));
                }
            }
            None => {
                return Err((
                    zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link,).into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        },
        None => {
            return Err((
                zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", link,).into(),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    }

    // Decrypt the cookie with the cyper
    let (cookie, mut ps_cookie) = Cookie::decrypt(encrypted, &manager.cipher)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Validate with the peer authenticators
    let mut open_syn_properties: EstablishmentProperties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };

    let mut is_shm = false;
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let mut att = pa
            .handle_open_syn(
                auth_link,
                &cookie,
                (
                    open_syn_properties.remove(pa.id().into()).map(|x| x.value),
                    ps_cookie.remove(pa.id().into()).map(|x| x.value),
                ),
            )
            .await;

        #[cfg(feature = "shared-memory")]
        if pa.id() == PeerAuthenticatorId::Shm {
            // Check if SHM has been validated from the other side
            att = match att {
                Ok(att) => {
                    is_shm = true;
                    Ok(att)
                }
                Err(e) => {
                    if e.is::<zenoh_util::core::zresult::ShmError>() {
                        is_shm = false;
                        Ok(None)
                    } else {
                        Err(e)
                    }
                }
            };
        }

        let mut att = att.map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        if let Some(att) = att.take() {
            ps_attachment
                .insert(Property {
                    key: pa.id().into(),
                    value: att,
                })
                .map_err(|e| (e, None))?;
        }
    }

    let output = AcceptOpenSynOutput {
        cookie,
        initial_sn: open_syn_initial_sn,
        lease: open_syn_lease,
        is_shm,
        open_ack_attachment: attachment_from_properties(&ps_attachment).ok(),
    };
    Ok(output)
}

// Validate the OpenSyn cookie and eventually initialize a new transport
struct AcceptInitTransportOutput {
    transport: TransportUnicast,
    initial_sn: ZInt,
    lease: Duration,
    open_ack_attachment: Option<Attachment>,
}
async fn accept_init_transport(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptOpenSynOutput,
) -> IResult<AcceptInitTransportOutput> {
    // Initialize the transport if it is new
    // NOTE: Keep the lock on the manager.opened and use it to protect concurrent
    //       addition of new transports and links
    let mut guard = zasynclock!(manager.state.unicast.opened);

    let open_ack_initial_sn = match guard.get(&input.cookie.pid) {
        Some(opened) => {
            if opened.sn_resolution != input.cookie.sn_resolution {
                return Err((
                    zerror!(
                        "Rejecting OpenSyn cookie on {} for peer: {}. Invalid sn resolution: {}. Expected: {}",
                        link, input.cookie.pid, input.cookie.sn_resolution, opened.sn_resolution
                    ).into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }

            if opened.whatami != input.cookie.whatami {
                return Err((
                    zerror!(
                        "Rejecting OpenSyn cookie on: {}. Invalid whatami: {}",
                        link,
                        input.cookie.pid
                    )
                    .into(),
                    Some(tmsg::close_reason::INVALID),
                ));
            }

            opened.initial_sn
        }
        None => {
            let initial_sn = zasynclock!(manager.prng).gen_range(0..input.cookie.sn_resolution);
            guard.insert(
                input.cookie.pid,
                Opened {
                    whatami: input.cookie.whatami,
                    sn_resolution: input.cookie.sn_resolution,
                    initial_sn,
                },
            );
            initial_sn
        }
    };

    let config = TransportConfigUnicast {
        peer: input.cookie.pid,
        whatami: input.cookie.whatami,
        sn_resolution: input.cookie.sn_resolution,
        initial_sn_tx: open_ack_initial_sn,
        initial_sn_rx: input.initial_sn,
        is_shm: input.is_shm,
        is_qos: input.cookie.is_qos,
    };
    let transport = manager
        .init_transport_unicast(config)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;

    // Retrieve the transport's transport
    let t = transport.get_transport().map_err(|e| (e, None))?;
    let _ = t
        .add_link(link.clone())
        .map_err(|e| (e, Some(tmsg::close_reason::GENERIC)))?;

    log::debug!(
        "New transport link established from {}: {}",
        input.cookie.pid,
        link
    );

    let output = AcceptInitTransportOutput {
        transport,
        initial_sn: open_ack_initial_sn,
        lease: input.lease,
        open_ack_attachment: input.open_ack_attachment,
    };
    Ok(output)
}

// Send an OpenAck
struct AcceptOpenAckOutput {
    transport: TransportUnicast,
    lease: Duration,
}
async fn accept_send_open_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptInitTransportOutput,
) -> ZResult<AcceptOpenAckOutput> {
    // Build OpenAck message
    let mut message = TransportMessage::make_open_ack(
        manager.config.unicast.lease,
        input.initial_sn,
        input.open_ack_attachment,
    );

    // Send the message on the link
    let _ = link.write_transport_message(&mut message).await?;

    let output = AcceptOpenAckOutput {
        transport: input.transport,
        lease: input.lease,
    };
    Ok(output)
}

// Notify the callback and start the link tasks
async fn accept_finalize_transport(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptOpenAckOutput,
) -> ZResult<()> {
    // Retrive the transport's transport
    let transport = input.transport.get_transport()?;

    // Acquire the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = transport.get_alive().await;
    if *a_guard {
        // Add the link to the transport
        // Compute a suitable keep alive interval based on the lease
        // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
        //       set the actual keep_alive timeout to one fourth of the agreed transport lease.
        //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
        //       check which considers a link as failed when no messages are received in 3.5 times the
        //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
        //       transport lease.
        let keep_alive = manager.config.unicast.keep_alive.min(input.lease / 4);
        // Start the TX loop
        let _ = transport.start_tx(link, keep_alive, manager.config.batch_size)?;

        // Assign a callback if the transport is new
        loop {
            match transport.get_callback() {
                Some(callback) => {
                    // Notify the transport handler there is a new link on this transport
                    callback.new_link(Link::from(link));
                    break;
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
                    transport.set_callback(callback);
                }
            }
        }

        // Start the RX loop
        let _ = transport.start_rx(link, input.lease)?;
    }
    drop(a_guard);

    Ok(())
}

async fn accept_link_stages(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> IResult<AcceptInitTransportOutput> {
    let output = accept_recv_init_syn(manager, link, auth_link).await?;
    let output = accept_send_init_ack(manager, link, auth_link, output).await?;
    let output = accept_recv_open_syn(manager, link, auth_link, output).await?;
    accept_init_transport(manager, link, auth_link, output).await
}

async fn accept_transport_stages(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    input: AcceptInitTransportOutput,
) -> ZResult<()> {
    let output = accept_send_open_ack(manager, link, auth_link, input).await?;
    accept_finalize_transport(manager, link, auth_link, output).await
}

pub(crate) async fn accept_link(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> ZResult<()> {
    let res = accept_link_stages(manager, link, auth_link).await;
    let output = match res {
        Ok(out) => out,
        Err((e, reason)) => {
            close_link(manager, link, auth_link, reason).await;
            return Err(e);
        }
    };

    let transport = output.transport.clone();
    let res = accept_transport_stages(manager, link, auth_link, output).await;
    if let Err(e) = res {
        let _ = transport
            .get_transport()?
            .close(tmsg::close_reason::GENERIC)
            .await;
        return Err(e);
    }

    Ok(())
}
