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
use super::super::TransportManager;
use super::authenticator::{
    AuthenticatedPeerLink, AuthenticatedPeerTransport, PeerAuthenticatorOutput,
};
use super::manager::Opened;
use super::protocol::core::{PeerId, Property, WhatAmI, ZInt};
use super::protocol::io::{WBuf, ZBuf, ZSlice};
use super::protocol::proto::{
    tmsg, Attachment, Close, InitAck, InitSyn, OpenAck, OpenSyn, TransportBody, TransportMessage,
};
use super::{TransportConfigUnicast, TransportUnicast};
use crate::net::link::LinkUnicast;
use rand::Rng;
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::hmac;
use zenoh_util::{zasynclock, zerror, zerror2};

type IError = (ZError, Option<u8>);
type IResult<T> = Result<T, IError>;

const WBUF_SIZE: usize = 64;

/*************************************/
/*              UTILS                */
/*************************************/
fn attachment_from_properties(ps: &[Property]) -> ZResult<Attachment> {
    if ps.is_empty() {
        let e = "Can not create an attachment with zero properties".to_string();
        zerror!(ZErrorKind::Other { descr: e })
    } else {
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_properties(ps);
        let zbuf: ZBuf = wbuf.into();
        let attachment = Attachment::new(zbuf);
        Ok(attachment)
    }
}

fn properties_from_attachment(mut att: Attachment) -> ZResult<Vec<Property>> {
    att.buffer.read_properties().ok_or_else(|| {
        let e = "Error while decoding attachment properties".to_string();
        zerror2!(ZErrorKind::Other { descr: e })
    })
}

/*************************************/
/*             COOKIE                */
/*************************************/
struct Cookie {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    is_qos: bool,
    nonce: ZInt,
}

impl WBuf {
    fn write_cookie(&mut self, cookie: &Cookie) -> bool {
        zcheck!(self.write_zint(cookie.whatami));
        zcheck!(self.write_peerid(&cookie.pid));
        zcheck!(self.write_zint(cookie.sn_resolution));
        zcheck!(self.write(if cookie.is_qos { 1 } else { 0 }));
        zcheck!(self.write_zint(cookie.nonce));
        true
    }
}

impl ZBuf {
    fn read_cookie(&mut self) -> Option<Cookie> {
        let whatami = self.read_zint()?;
        let pid = self.read_peerid()?;
        let sn_resolution = self.read_zint()?;
        let is_qos = self.read()? == 1;
        let nonce = self.read_zint()?;

        Some(Cookie {
            whatami,
            pid,
            sn_resolution,
            is_qos,
            nonce,
        })
    }
}

async fn close_link(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    mut reason: Option<u8>,
) {
    if let Some(reason) = reason.take() {
        // Build the close message
        let peer_id = Some(manager.config.pid.clone());
        let link_only = true;
        let attachment = None;
        let message = TransportMessage::make_close(peer_id, reason, link_only, attachment);
        // Send the close message on the link
        let _ = link.write_transport_message(message).await;
    }
    // Close the link
    let _ = link.close().await;
    // Notify the authenticators
    for pa in manager.config.unicast.peer_authenticator.iter() {
        pa.handle_link_err(auth_link).await;
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
struct OpenInitSynOutput {
    sn_resolution: ZInt,
    auth_transport: AuthenticatedPeerTransport,
}
async fn open_send_init_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<OpenInitSynOutput> {
    let mut auth = PeerAuthenticatorOutput::default();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let ps = pa
            .get_init_syn_properties(auth_link, &manager.config.pid)
            .await
            .map_err(|e| (e, None))?;
        auth = auth.merge(ps);
    }

    // Build and send an InitSyn Message
    let init_syn_version = manager.config.version;
    let init_syn_whatami = manager.config.whatami;
    let init_syn_pid = manager.config.pid.clone();
    let init_syn_sn_resolution = manager.config.sn_resolution;
    let init_syn_qos = true;
    let init_syn_attachment = attachment_from_properties(&auth.properties).ok();

    // Build and send the InitSyn message
    let message = TransportMessage::make_init_syn(
        init_syn_version,
        init_syn_whatami,
        init_syn_pid,
        init_syn_sn_resolution,
        init_syn_qos,
        init_syn_attachment,
    );
    let _ = link
        .write_transport_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenInitSynOutput {
        sn_resolution: manager.config.sn_resolution,
        auth_transport: auth.transport,
    };
    Ok(output)
}

struct OpenInitAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    is_qos: bool,
    initial_sn_tx: ZInt,
    cookie: ZSlice,
    open_syn_attachment: Option<Attachment>,
    auth_transport: AuthenticatedPeerTransport,
}
async fn open_recv_init_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
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
    let (init_ack_whatami, init_ack_pid, init_ack_sn_resolution, init_ack_is_qos, init_ack_cookie) =
        match msg.body {
            TransportBody::InitAck(InitAck {
                whatami,
                pid,
                sn_resolution,
                is_qos,
                cookie,
            }) => (whatami, pid, sn_resolution, is_qos, cookie),
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

    // Check if a transport is already open with the target peer
    let mut guard = zasynclock!(manager.state.unicast.opened);
    let (sn_resolution, initial_sn_tx, is_opened) = if let Some(s) = guard.get(&init_ack_pid) {
        if let Some(sn_resolution) = init_ack_sn_resolution {
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
        let sn_resolution = match init_ack_sn_resolution {
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

    let init_ack_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => vec![],
    };

    let mut auth = PeerAuthenticatorOutput {
        transport: input.auth_transport,
        ..Default::default()
    };
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let ps = pa
            .handle_init_ack(
                auth_link,
                &init_ack_pid,
                sn_resolution,
                &init_ack_properties,
            )
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        auth = auth.merge(ps);
    }

    if !is_opened {
        // Store the data
        guard.insert(
            init_ack_pid.clone(),
            Opened {
                whatami: init_ack_whatami,
                sn_resolution,
                initial_sn: initial_sn_tx,
            },
        );
    }
    drop(guard);

    let output = OpenInitAckOutput {
        pid: init_ack_pid,
        whatami: init_ack_whatami,
        sn_resolution,
        is_qos: init_ack_is_qos,
        initial_sn_tx,
        cookie: init_ack_cookie,
        open_syn_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_transport: auth.transport,
    };
    Ok(output)
}

struct OpenOpenSynOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn_tx: ZInt,
    is_qos: bool,
    auth_transport: AuthenticatedPeerTransport,
}
async fn open_send_open_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: OpenInitAckOutput,
) -> IResult<OpenOpenSynOutput> {
    // Build and send an OpenSyn message
    let lease = manager.config.unicast.lease;
    let message = TransportMessage::make_open_syn(
        lease,
        input.initial_sn_tx,
        input.cookie,
        input.open_syn_attachment,
    );
    let _ = link
        .write_transport_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenOpenSynOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        initial_sn_tx: input.initial_sn_tx,
        is_qos: input.is_qos,
        auth_transport: input.auth_transport,
    };
    Ok(output)
}

struct OpenAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    is_qos: bool,
    initial_sn_tx: ZInt,
    initial_sn_rx: ZInt,
    lease: Duration,
    auth_transport: AuthenticatedPeerTransport,
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

    let opean_ack_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let _ = pa
            .handle_open_ack(auth_link, &opean_ack_properties)
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
    }

    let output = OpenAckOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        is_qos: input.is_qos,
        initial_sn_tx: input.initial_sn_tx,
        initial_sn_rx,
        lease,
        auth_transport: input.auth_transport,
    };
    Ok(output)
}

async fn open_stages(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
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
    let auth_link = AuthenticatedPeerLink {
        src: link.get_src(),
        dst: link.get_src(),
        peer_id: None,
        properties: None,
    };

    let res = open_stages(manager, link, &auth_link).await;
    let info = match res {
        Ok(v) => v,
        Err((e, reason)) => {
            let _ = close_link(manager, link, &auth_link, reason).await;
            return Err(e);
        }
    };

    let config = TransportConfigUnicast {
        peer: info.pid.clone(),
        whatami: info.whatami,
        sn_resolution: info.sn_resolution,
        initial_sn_tx: info.initial_sn_tx,
        initial_sn_rx: info.initial_sn_rx,
        is_shm: info.auth_transport.is_shm,
        is_qos: info.is_qos,
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
        let keep_alive = Duration::from_millis(
            (manager.config.unicast.keep_alive.as_millis() as ZInt)
                .min(info.lease.as_millis() as ZInt / 4),
        );
        let _ = t.add_link(link.clone())?;

        // Start the TX loop
        let _ = t.start_tx(link, keep_alive, manager.config.batch_size)?;

        // Assign a callback if the transport is new
        loop {
            match t.get_callback() {
                Some(callback) => {
                    // Notify the transport handler there is a new link on this transport
                    callback.new_link(link.clone());
                    break;
                }
                None => {
                    // Notify the transport handler that there is a new transport and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_transport() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to.
                    let callback = manager.config.handler.new_unicast(transport.clone())?;
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

/*************************************/
/*             ACCEPT                */
/*************************************/

// Read and eventually accept an InitSyn
struct AcceptInitSynOutput {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    is_qos: bool,
    init_ack_attachment: Option<Attachment>,
    auth_transport: AuthenticatedPeerTransport,
}
async fn accept_recv_init_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<AcceptInitSynOutput> {
    // Wait to read an InitSyn
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single InitSyn on {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (init_syn_version, init_syn_whatami, init_syn_pid, init_syn_sn_resolution, init_syn_is_qos) =
        match msg.body {
            TransportBody::InitSyn(InitSyn {
                version,
                whatami,
                pid,
                sn_resolution,
                is_qos,
            }) => (version, whatami, pid, sn_resolution, is_qos),
            _ => {
                let e = format!(
                    "Received invalid message instead of an InitSyn on {}: {:?}",
                    link, msg.body
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        };

    // Check if we are allowed to open more links if the transport is established
    if let Some(t) = manager.get_transport_unicast(&init_syn_pid) {
        // Check if we have reached maximum number of links for this transport
        let links = t.get_transport().map_err(|e| (e, None))?.get_links();
        if links.len() >= manager.config.unicast.max_links {
            let e = format!(
                "Rejecting Open on {} because of maximum links ({}) limit reached for peer: {}",
                manager.config.unicast.max_links, link, init_syn_pid
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    }

    // Check if the version is supported
    if init_syn_version > manager.config.version {
        let e = format!(
            "Rejecting InitSyn on {} because of unsupported Zenoh version from peer: {}",
            link, init_syn_pid
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(tmsg::close_reason::INVALID),
        ));
    }

    // Validate the InitSyn with the peer authenticators
    let init_syn_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    let mut auth = PeerAuthenticatorOutput::default();
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let ps = pa
            .handle_init_syn(
                auth_link,
                &init_syn_pid,
                init_syn_sn_resolution,
                &init_syn_properties,
            )
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        auth = auth.merge(ps);
    }

    let output = AcceptInitSynOutput {
        whatami: init_syn_whatami,
        pid: init_syn_pid,
        sn_resolution: init_syn_sn_resolution,
        is_qos: init_syn_is_qos,
        init_ack_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_transport: auth.transport,
    };
    Ok(output)
}

// Send an InitAck
struct AcceptInitAckOutput {
    auth_transport: AuthenticatedPeerTransport,
}
async fn accept_send_init_ack(
    manager: &TransportManager,
    link: &LinkUnicast,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptInitSynOutput,
) -> IResult<AcceptInitAckOutput> {
    // Compute the minimum SN Resolution
    let agreed_sn_resolution = manager.config.sn_resolution.min(input.sn_resolution);

    // Create and encode the cookie
    let mut wbuf = WBuf::new(64, false);
    let cookie = Cookie {
        whatami: input.whatami,
        pid: input.pid.clone(),
        sn_resolution: agreed_sn_resolution,
        is_qos: input.is_qos,
        nonce: zasynclock!(manager.prng).gen_range(0..agreed_sn_resolution),
    };
    wbuf.write_cookie(&cookie);

    // Build the fields for the InitAck message
    let whatami = manager.config.whatami;
    let apid = manager.config.pid.clone();
    let sn_resolution = if agreed_sn_resolution == input.sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };

    // Use the BlockCipher to encrypt the cookie
    let serialized = ZBuf::from(wbuf).to_vec();
    let mut guard = zasynclock!(manager.prng);
    let encrypted = manager.cipher.encrypt(serialized, &mut *guard);
    drop(guard);

    // Compute and store cookie hash
    let hash = hmac::digest(&encrypted);
    zasynclock!(manager.state.unicast.incoming).insert(link.clone(), Some(hash));

    // Send the cookie
    let cookie: ZSlice = encrypted.into();
    let message = TransportMessage::make_init_ack(
        whatami,
        apid,
        sn_resolution,
        input.is_qos,
        cookie,
        input.init_ack_attachment,
    );

    // Send the message on the link
    let _ = link
        .write_transport_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = AcceptInitAckOutput {
        auth_transport: input.auth_transport,
    };
    Ok(output)
}

// Read and eventually accept an OpenSyn
struct AcceptOpenSynOutput {
    cookie: Cookie,
    initial_sn: ZInt,
    lease: Duration,
    open_ack_attachment: Option<Attachment>,
    auth_transport: AuthenticatedPeerTransport,
}
async fn accept_recv_open_syn(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    input: AcceptInitAckOutput,
) -> IResult<AcceptOpenSynOutput> {
    // Wait to read an OpenSyn
    let mut messages = link.read_transport_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single OpenSyn on {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
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
            let e = format!(
                "Received a close message (reason {}) instead of an OpenSyn on: {:?}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received invalid message instead of an OpenSyn on {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
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
                    let e = format!("Rejecting OpenSyn on: {}. Unkwown cookie.", link);
                    return Err((
                        zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                        Some(tmsg::close_reason::INVALID),
                    ));
                }
            }
            None => {
                let e = format!("Rejecting OpenSyn on: {}. Unkwown cookie.", link,);
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(tmsg::close_reason::INVALID),
                ));
            }
        },
        None => {
            let e = format!("Rejecting OpenSyn on: {}. Unkwown cookie.", link,);
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    }

    // Decrypt the cookie with the cyper
    let decrypted = manager
        .cipher
        .decrypt(encrypted)
        .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
    let mut open_syn_cookie = ZBuf::from(decrypted);

    // Verify the cookie
    let cookie = match open_syn_cookie.read_cookie() {
        Some(ck) => ck,
        None => {
            let e = format!("Rejecting OpenSyn on: {}. Invalid cookie.", link,);
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(tmsg::close_reason::INVALID),
            ));
        }
    };

    // Validate with the peer authenticators
    let open_syn_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    let mut auth = PeerAuthenticatorOutput {
        transport: input.auth_transport,
        ..Default::default()
    };
    for pa in manager.config.unicast.peer_authenticator.iter() {
        let ps = pa
            .handle_open_syn(auth_link, &open_syn_properties)
            .await
            .map_err(|e| (e, Some(tmsg::close_reason::INVALID)))?;
        auth = auth.merge(ps);
    }

    let output = AcceptOpenSynOutput {
        cookie,
        initial_sn: open_syn_initial_sn,
        lease: open_syn_lease,
        open_ack_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_transport: auth.transport,
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
                let e = format!(
                "Rejecting OpenSyn cookie on {} for peer: {}. Invalid sn resolution: {}. Expected: {}",
                link, input.cookie.pid, input.cookie.sn_resolution, opened.sn_resolution
            );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(tmsg::close_reason::INVALID),
                ));
            }

            if opened.whatami != input.cookie.whatami {
                let e = format!(
                    "Rejecting OpenSyn cookie on: {}. Invalid whatami: {}",
                    link, input.cookie.pid
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(tmsg::close_reason::INVALID),
                ));
            }

            opened.initial_sn
        }
        None => {
            let initial_sn = zasynclock!(manager.prng).gen_range(0..input.cookie.sn_resolution);
            guard.insert(
                input.cookie.pid.clone(),
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
        peer: input.cookie.pid.clone(),
        whatami: input.cookie.whatami,
        sn_resolution: input.cookie.sn_resolution,
        initial_sn_tx: open_ack_initial_sn,
        initial_sn_rx: input.initial_sn,
        is_shm: input.auth_transport.is_shm,
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
    let message = TransportMessage::make_open_ack(
        manager.config.unicast.lease,
        input.initial_sn,
        input.open_ack_attachment,
    );

    // Send the message on the link
    let _ = link.write_transport_message(message).await?;

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
        let keep_alive = Duration::from_millis(
            (manager.config.unicast.keep_alive.as_millis() as ZInt)
                .min(input.lease.as_millis() as ZInt / 4),
        );
        // Start the TX loop
        let _ = transport.start_tx(link, keep_alive, manager.config.batch_size)?;

        // Assign a callback if the transport is new
        loop {
            match transport.get_callback() {
                Some(callback) => {
                    // Notify the transport handler there is a new link on this transport
                    callback.new_link(link.clone());
                    break;
                }
                None => {
                    // Notify the transport handler that there is a new transport and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_transport() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to.
                    let callback = manager
                        .config
                        .handler
                        .new_unicast(input.transport.clone())
                        .map_err(|e| {
                            let e = format!(
                                "Rejecting OpenSyn on: {}. New transport error: {:?}",
                                link, e
                            );
                            zerror2!(ZErrorKind::InvalidSession { descr: e })
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
    auth_link: &AuthenticatedPeerLink,
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
    auth_link: &AuthenticatedPeerLink,
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
