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
use super::authenticator::AuthenticatedPeerLink;
use super::{attachment_from_properties, close_link, properties_from_attachment};
use super::{TransportConfigUnicast, TransportUnicast};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::core::{PeerId, WhatAmI, ZInt};
use crate::net::protocol::io::ZSlice;
use crate::net::protocol::proto::{
    tmsg, Attachment, Close, OpenAck, TransportBody, TransportMessage,
};
use crate::net::transport::unicast::establishment::authenticator::PeerAuthenticatorId;
use crate::net::transport::unicast::establishment::EstablishmentProperties;
use crate::net::transport::unicast::manager::Opened;
use crate::net::transport::{TransportManager, TransportPeer};
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zerror2};

type IError = (ZError, Option<u8>);
type IResult<T> = Result<T, IError>;

/*************************************/
/*              OPEN                 */
/*************************************/
struct OpenInitSynOutput {
    sn_resolution: ZInt,
}
async fn open_send_init_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> IResult<OpenInitSynOutput> {
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let mut att = pa
            .get_init_syn_properties(auth_link, &manager.config.pid)
            .await
            .map_err(|e| (e, None))?;
        if let Some(att) = att.take() {
            ps_attachment.insert(att).map_err(|e| (e, None))?;
        }
    }

    // Build and send the InitSyn message
    let mut message = TransportMessage::make_init_syn(
        manager.config.version,
        manager.config.whatami,
        manager.config.pid,
        manager.config.sn_resolution,
        manager.config.unicast.is_qos,
        attachment_from_properties(&ps_attachment).ok(),
    );
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenInitSynOutput {
        sn_resolution: manager.config.sn_resolution,
    };
    Ok(output)
}

struct OpenInitAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    is_qos: bool,
    is_shm: bool,
    initial_sn_tx: ZInt,
    cookie: ZSlice,
    open_syn_attachment: Option<Attachment>,
}
async fn open_recv_init_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
    input: OpenInitSynOutput,
) -> IResult<OpenInitAckOutput> {
    // Wait to read an InitAck
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages in response to an InitSyn on {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let init_ack = match msg.body {
        TransportBody::InitAck(init_ack) => init_ack,
        TransportBody::Close(Close { reason, .. }) => {
            let e = format!(
                "Received a close message (reason {}) in response to an InitSyn on: {}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received an invalid message in response to an InitSyn on {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };

    // Store the peer id associate do this link
    auth_link.peer_id = Some(init_ack.pid);

    // Check if a transport is already open with the target peer
    let mut guard = zasynclock!(manager.state.unicast.opened);
    let (sn_resolution, initial_sn_tx, is_opened) = if let Some(s) = guard.get(&init_ack.pid) {
        if let Some(sn_resolution) = init_ack.sn_resolution {
            if sn_resolution != s.sn_resolution {
                let e = format!(
                    "Rejecting InitAck on {}. Invalid sn resolution: {}",
                    link, sn_resolution
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        }
        (s.sn_resolution, s.initial_sn, true)
    } else {
        let sn_resolution = match init_ack.sn_resolution {
            Some(sn_resolution) => {
                if sn_resolution > input.sn_resolution {
                    let e = format!(
                        "Rejecting InitAck on {}. Invalid sn resolution: {}",
                        link, sn_resolution
                    );
                    return Err((
                        zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                        Some(tmsg::close_reason::INVALID),
                    ));
                }
                sn_resolution
            }
            None => input.sn_resolution,
        };
        let initial_sn_tx = zasynclock!(manager.prng).gen_range(0..sn_resolution);
        (sn_resolution, initial_sn_tx, false)
    };

    let mut init_ack_properties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };

    let mut is_shm = false;
    let mut ps_attachment = EstablishmentProperties::new();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let mut att = pa
            .handle_init_ack(
                auth_link,
                &init_ack.pid,
                sn_resolution,
                init_ack_properties.remove(pa.id() as ZInt),
            )
            .await;

        #[cfg(feature = "zero-copy")]
        if pa.id() == PeerAuthenticatorId::Shm {
            // Check if SHM has been validated from the other side
            att = match att {
                Ok(att) => {
                    is_shm = att.is_some();
                    Ok(att)
                }
                Err(e) => match e.get_kind() {
                    ZErrorKind::SharedMemory { .. } => {
                        is_shm = false;
                        Ok(None)
                    }
                    _ => Err(e),
                },
            };
        }

        let mut att = att.map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        if let Some(att) = att.take() {
            ps_attachment.insert(att).map_err(|e| (e, None))?;
        }
    }

    if !is_opened {
        // Store the data
        guard.insert(
            init_ack.pid,
            Opened {
                whatami: init_ack.whatami,
                sn_resolution,
                initial_sn: initial_sn_tx,
            },
        );
    }
    drop(guard);

    let output = OpenInitAckOutput {
        pid: init_ack.pid,
        whatami: init_ack.whatami,
        sn_resolution,
        is_qos: init_ack.is_qos,
        is_shm,
        initial_sn_tx,
        cookie: init_ack.cookie,
        open_syn_attachment: attachment_from_properties(&ps_attachment).ok(),
    };
    Ok(output)
}

struct OpenOpenSynOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn_tx: ZInt,
    is_qos: bool,
    is_shm: bool,
}
async fn open_send_open_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: OpenInitAckOutput,
) -> IResult<OpenOpenSynOutput> {
    // Build and send an OpenSyn message
    let lease = manager.config.unicast.lease;
    let mut message = TransportMessage::make_open_syn(
        lease,
        input.initial_sn_tx,
        input.cookie,
        input.open_syn_attachment,
    );
    let _ = link
        .write_transport_message(&mut message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenOpenSynOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        initial_sn_tx: input.initial_sn_tx,
        is_qos: input.is_qos,
        is_shm: input.is_shm,
    };
    Ok(output)
}

struct OpenAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    is_qos: bool,
    is_shm: bool,
    initial_sn_tx: ZInt,
    initial_sn_rx: ZInt,
    lease: Duration,
}
async fn open_recv_open_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    input: OpenOpenSynOutput,
) -> IResult<OpenAckOutput> {
    // Wait to read an OpenAck
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages in response to an InitSyn on {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (lease, initial_sn_rx) = match msg.body {
        TransportBody::OpenAck(OpenAck { lease, initial_sn }) => (lease, initial_sn),
        TransportBody::Close(Close { reason, .. }) => {
            let e = format!(
                "Received a close message (reason {}) in response to an OpenSyn on: {:?}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received an invalid message in response to an OpenSyn on {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };

    let mut opean_ack_properties = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => EstablishmentProperties::new(),
    };
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let _ = pa
            .handle_open_ack(auth_link, opean_ack_properties.remove(pa.id() as ZInt))
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
    }

    let output = OpenAckOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        is_qos: input.is_qos,
        is_shm: input.is_shm,
        initial_sn_tx: input.initial_sn_tx,
        initial_sn_rx,
        lease,
    };
    Ok(output)
}

async fn open_stages(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &mut AuthenticatedPeerLink,
) -> IResult<OpenAckOutput> {
    let output = open_send_init_syn(manager, link, auth_link).await?;
    let output = open_recv_init_ack(manager, link, auth_link, output).await?;
    let output = open_send_open_syn(manager, link, auth_link, output).await?;
    open_recv_open_ack(manager, link, auth_link, output).await
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

    let res = open_stages(manager, link, &mut auth_link).await;
    let info = match res {
        Ok(v) => v,
        Err((e, reason)) => {
            let _ = close_link(manager, link, &auth_link, reason).await;
            return Err(e);
        }
    };

    let config = TransportConfigUnicast {
        peer: info.pid,
        whatami: info.whatami,
        sn_resolution: info.sn_resolution,
        initial_sn_tx: info.initial_sn_tx,
        initial_sn_rx: info.initial_sn_rx,
        is_qos: info.is_qos,
        is_shm: info.is_shm,
    };
    let res = manager.init_transport_unicast(config);
    let transport = match res {
        Ok(s) => s,
        Err(e) => {
            let _ = close_link(manager, link, &auth_link, Some(tmsg::close_reason::INVALID)).await;
            return Err(e);
        }
    };

    // Retrive the transport's transport
    let t = transport.get_transport()?;

    // Acquire the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = t.get_alive().await;
    if *a_guard {
        // Compute a suitable keep alive interval based on the lease
        // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
        //       set the actual keep_alive timeout to one fourth of the agreed transport lease.
        //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
        //       check which considers a link as failed when no messages are received in 3.5 times the
        //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
        //       transport lease.
        let keep_alive = manager.config.unicast.keep_alive.min(info.lease / 4);
        let _ = t.add_link(link.clone())?;

        // Start the TX loop
        let _ = t.start_tx(link, keep_alive, manager.config.batch_size)?;

        // Assign a callback if the transport is new
        loop {
            match t.get_callback() {
                Some(callback) => {
                    // Notify the transport handler there is a new link on this transport
                    callback.new_link(Link::from(link));
                    break;
                }
                None => {
                    let peer = TransportPeer {
                        pid: info.pid,
                        whatami: info.whatami,
                        is_qos: info.is_qos,
                        is_shm: info.is_shm,
                        links: vec![Link::from(link)],
                    };
                    // Notify the transport handler that there is a new transport and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_transport() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to.
                    let callback = manager
                        .config
                        .handler
                        .new_unicast(peer, transport.clone())?;
                    // Set the callback on the transport
                    let _ = t.set_callback(callback);
                }
            }
        }

        // Start the RX loop
        let _ = t.start_rx(link, info.lease)?;
    }
    drop(a_guard);

    zasynclock!(manager.state.unicast.opened).remove(&info.pid);

    Ok(transport)
}
