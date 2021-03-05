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
use super::authenticator::{
    AuthenticatedPeerLink, AuthenticatedPeerSession, PeerAuthenticatorOutput,
};
use super::core::{PeerId, Property, WhatAmI, ZInt};
use super::defaults::SESSION_SEQ_NUM_RESOLUTION;
use super::io::{RBuf, WBuf};
use super::link::{Link, Locator};
use super::proto::{
    smsg, Attachment, Close, InitAck, InitSyn, OpenAck, OpenSyn, SessionBody, SessionMessage,
};
use super::{Opened, Session, SessionManager};
use rand::Rng;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zerror};

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
        let rbuf: RBuf = wbuf.into();
        let attachment = Attachment::make(smsg::attachment::PROPERTIES, rbuf);
        Ok(attachment)
    }
}

fn properties_from_attachment(mut att: Attachment) -> ZResult<Vec<Property>> {
    if att.encoding != smsg::attachment::PROPERTIES {
        let e = format!(
            "Invalid attachment encoding for properties: {}",
            att.encoding
        );
        return zerror!(ZErrorKind::Other { descr: e });
    }

    let res = att.buffer.read_properties();
    match res {
        Some(ps) => Ok(ps),
        None => {
            let e = "Error while decoding properties".to_string();
            zerror!(ZErrorKind::Other { descr: e })
        }
    }
}

/*************************************/
/*             COOKIE                */
/*************************************/
struct Cookie {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    src: Locator,
    dst: Locator,
    nonce: ZInt,
}

impl WBuf {
    fn write_cookie(&mut self, cookie: &Cookie) -> bool {
        zcheck!(self.write_zint(cookie.whatami));
        zcheck!(self.write_peerid(&cookie.pid));
        zcheck!(self.write_zint(cookie.sn_resolution));
        zcheck!(self.write_string(&cookie.src.to_string()));
        zcheck!(self.write_string(&cookie.dst.to_string()));
        zcheck!(self.write_zint(cookie.nonce));
        true
    }
}

impl RBuf {
    fn read_cookie(&mut self) -> Option<Cookie> {
        let whatami = self.read_zint()?;
        let pid = self.read_peerid()?;
        let sn_resolution = self.read_zint()?;
        let src: Locator = self.read_locator()?;
        let dst: Locator = self.read_locator()?;
        let nonce = self.read_zint()?;

        Some(Cookie {
            whatami,
            pid,
            sn_resolution,
            src,
            dst,
            nonce,
        })
    }
}

async fn close_link(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
    mut reason: Option<u8>,
) {
    if let Some(reason) = reason.take() {
        // Build the close message
        let peer_id = Some(manager.config.pid.clone());
        let link_only = true;
        let attachment = None;
        let message = SessionMessage::make_close(peer_id, reason, link_only, attachment);
        // Send the close message on the link
        let _ = link.write_session_message(message).await;
    }
    // Close the link
    let _ = link.close().await;
    // Notify the authenticators
    for pa in manager.config.peer_authenticator.iter() {
        pa.handle_link_err(auth_link).await;
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
struct OpenInitSynOutput {
    sn_resolution: ZInt,
    auth_session: AuthenticatedPeerSession,
}
async fn open_send_init_syn(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<OpenInitSynOutput> {
    let mut auth = PeerAuthenticatorOutput::default();
    for pa in manager.config.peer_authenticator.iter() {
        let ps = pa
            .get_init_syn_properties(&auth_link, &manager.config.pid)
            .await
            .map_err(|e| (e, None))?;
        auth = auth.merge(ps);
    }

    // Build and send an InitSyn Message
    let init_syn_version = manager.config.version;
    let init_syn_whatami = manager.config.whatami;
    let init_syn_pid = manager.config.pid.clone();
    let init_syn_sn_resolution = if manager.config.sn_resolution == *SESSION_SEQ_NUM_RESOLUTION {
        None
    } else {
        Some(manager.config.sn_resolution)
    };
    let init_syn_attachment = attachment_from_properties(&auth.properties).ok();

    // Build and send the InitSyn message
    let message = SessionMessage::make_init_syn(
        init_syn_version,
        init_syn_whatami,
        init_syn_pid,
        init_syn_sn_resolution,
        init_syn_attachment,
    );
    let _ = link
        .write_session_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenInitSynOutput {
        sn_resolution: manager.config.sn_resolution,
        auth_session: auth.session,
    };
    Ok(output)
}

struct OpenInitAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn_tx: ZInt,
    cookie: RBuf,
    open_syn_attachment: Option<Attachment>,
    auth_session: AuthenticatedPeerSession,
}
async fn open_recv_init_ack(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
    input: OpenInitSynOutput,
) -> IResult<OpenInitAckOutput> {
    // Wait to read an InitAck
    let mut messages = link.read_session_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages in response to an InitSyn on link {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(smsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (init_ack_whatami, init_ack_pid, init_ack_sn_resolution, init_ack_cookie) = match msg.body {
        SessionBody::InitAck(InitAck {
            whatami,
            pid,
            sn_resolution,
            cookie,
        }) => (whatami, pid, sn_resolution, cookie),
        SessionBody::Close(Close { reason, .. }) => {
            let e = format!(
                "Received a close message (reason {}) in response to an InitSyn on link: {}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received an invalid message in response to an InitSyn on link {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
    };

    // Check if a session is already open with the target peer
    let mut guard = zasynclock!(manager.opened);
    let (sn_resolution, initial_sn_tx, is_opened) = if let Some(s) = guard.get(&init_ack_pid) {
        if let Some(sn_resolution) = init_ack_sn_resolution {
            if sn_resolution != s.sn_resolution {
                let e = format!(
                    "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                    link, init_ack_pid
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(smsg::close_reason::INVALID),
                ));
            }
        }
        (s.sn_resolution, s.initial_sn, true)
    } else {
        let sn_resolution = match init_ack_sn_resolution {
            Some(sn_resolution) => {
                if sn_resolution > input.sn_resolution {
                    let e = format!(
                        "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                        link, init_ack_pid
                    );
                    return Err((
                        zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                        Some(smsg::close_reason::INVALID),
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
            properties_from_attachment(att).map_err(|e| (e, Some(smsg::close_reason::INVALID)))?
        }
        None => vec![],
    };

    let mut auth = PeerAuthenticatorOutput {
        session: input.auth_session,
        ..Default::default()
    };
    for pa in manager.config.peer_authenticator.iter() {
        let ps = pa
            .handle_init_ack(
                &auth_link,
                &init_ack_pid,
                sn_resolution,
                &init_ack_properties,
            )
            .await
            .map_err(|e| (e, Some(smsg::close_reason::INVALID)))?;
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
        initial_sn_tx,
        cookie: init_ack_cookie,
        open_syn_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_session: auth.session,
    };
    Ok(output)
}

struct OpenOpenSynOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn_tx: ZInt,
    auth_session: AuthenticatedPeerSession,
}
async fn open_send_open_syn(
    manager: &SessionManager,
    link: &Link,
    _auth_link: &AuthenticatedPeerLink,
    input: OpenInitAckOutput,
) -> IResult<OpenOpenSynOutput> {
    // Build and send an OpenSyn message
    let lease = manager.config.lease;
    let message = SessionMessage::make_open_syn(
        lease,
        input.initial_sn_tx,
        input.cookie,
        input.open_syn_attachment,
    );
    let _ = link
        .write_session_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = OpenOpenSynOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        initial_sn_tx: input.initial_sn_tx,
        auth_session: input.auth_session,
    };
    Ok(output)
}

struct OpenAckOutput {
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn_tx: ZInt,
    initial_sn_rx: ZInt,
    lease: ZInt,
    auth_session: AuthenticatedPeerSession,
}
async fn open_recv_open_ack(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
    input: OpenOpenSynOutput,
) -> IResult<OpenAckOutput> {
    // Wait to read an OpenAck
    let mut messages = link.read_session_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages in response to an InitSyn on link {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(smsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (lease, initial_sn_rx) = match msg.body {
        SessionBody::OpenAck(OpenAck { lease, initial_sn }) => (lease, initial_sn),
        SessionBody::Close(Close { reason, .. }) => {
            let e = format!(
                "Received a close message (reason {}) in response to an OpenSyn on link: {:?}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received an invalid message in response to an OpenSyn on link {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
    };

    let opean_ack_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(smsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    for pa in manager.config.peer_authenticator.iter() {
        let _ = pa
            .handle_open_ack(&auth_link, &opean_ack_properties)
            .await
            .map_err(|e| (e, Some(smsg::close_reason::INVALID)))?;
    }

    let output = OpenAckOutput {
        pid: input.pid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        initial_sn_tx: input.initial_sn_tx,
        initial_sn_rx,
        lease,
        auth_session: input.auth_session,
    };
    Ok(output)
}

async fn open_stages(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<OpenAckOutput> {
    let output = open_send_init_syn(manager, link, auth_link).await?;
    let output = open_recv_init_ack(manager, link, auth_link, output).await?;
    let output = open_send_open_syn(manager, link, auth_link, output).await?;
    open_recv_open_ack(manager, link, auth_link, output).await
}

pub(super) async fn open_link(manager: &SessionManager, link: &Link) -> ZResult<Session> {
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

    let session = manager
        .get_or_new_session(
            &info.pid,
            &info.whatami,
            info.sn_resolution,
            info.initial_sn_tx,
            info.initial_sn_rx,
            info.auth_session.is_local,
        )
        .await;

    // Retrive the session's transport
    let transport = session.get_transport()?;

    // Acquire the lock to avoid concurrent new_session and closing/closed notifications
    let a_guard = transport.get_alive().await;
    if *a_guard {
        // Compute a suitable keep alive interval based on the lease
        // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
        //       set the actual keep_alive timeout to one fourth of the agreed session lease.
        //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
        //       check which considers a link as failed when no messages are received in 3.5 times the
        //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
        //       session lease.
        let keep_alive = manager.config.keep_alive.min(info.lease / 4);
        let _ = transport
            .add_link(
                link.clone(),
                manager.config.batch_size,
                info.lease,
                keep_alive,
            )
            .await?;

        // Start the TX loop
        let _ = transport.start_tx(&link).await?;

        // Assign a callback if the session is new
        loop {
            match transport.get_callback().await {
                Some(callback) => {
                    // Notify the session handler there is a new link on this session
                    callback.new_link(link.clone()).await;
                    break;
                }
                None => {
                    // Notify the session handler that there is a new session and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_session() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to.
                    let callback = manager.config.handler.new_session(session.clone()).await?;
                    // Set the callback on the transport
                    let _ = transport.set_callback(callback).await;
                }
            }
        }

        // Start the RX loop
        let _ = transport.start_rx(&link).await?;
    }
    drop(a_guard);

    let mut guard = zasynclock!(manager.opened);
    guard.remove(&info.pid);
    drop(guard);

    Ok(session)
}

/*************************************/
/*             ACCEPT                */
/*************************************/
struct AcceptInitSynOutput {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    init_ack_attachment: Option<Attachment>,
    auth_session: AuthenticatedPeerSession,
}
async fn accept_recv_init_syn(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<AcceptInitSynOutput> {
    // Wait to read an InitSyn
    let mut messages = link.read_session_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single InitSyn on link {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(smsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (init_syn_version, init_syn_whatami, init_syn_pid, init_syn_sn_resolution) = match msg.body
    {
        SessionBody::InitSyn(InitSyn {
            version,
            whatami,
            pid,
            sn_resolution,
        }) => (version, whatami, pid, sn_resolution),
        _ => {
            let e = format!(
                "Received invalid message instead of an InitSyn on link {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
    };

    // Check if we are allowed to open more links if the session is established
    if let Some(s) = manager.get_session(&init_syn_pid).await {
        // Check if we have reached maximum number of links for this session
        if let Some(limit) = manager.config.max_links {
            let links = s.get_links().await.map_err(|e| (e, None))?;
            if links.len() >= limit {
                let e = format!(
                    "Rejecting Open on link {} because of maximum links limit reached for peer: {}",
                    link, init_syn_pid
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                    Some(smsg::close_reason::INVALID),
                ));
            }
        }
    }

    // Check if the version is supported
    if init_syn_version > manager.config.version {
        let e = format!(
            "Rejecting InitSyn on link {} because of unsupported Zenoh version from peer: {}",
            link, init_syn_pid
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(smsg::close_reason::INVALID),
        ));
    }

    // Get the SN Resolution
    let init_syn_sn_resolution = if let Some(snr) = init_syn_sn_resolution {
        snr
    } else {
        *SESSION_SEQ_NUM_RESOLUTION
    };

    // Validate the InitSyn with the peer authenticators
    let init_syn_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(smsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    let mut auth = PeerAuthenticatorOutput::default();
    for pa in manager.config.peer_authenticator.iter() {
        let ps = pa
            .handle_init_syn(
                &auth_link,
                &init_syn_pid,
                init_syn_sn_resolution,
                &init_syn_properties,
            )
            .await
            .map_err(|e| (e, Some(smsg::close_reason::INVALID)))?;
        auth = auth.merge(ps);
    }

    let output = AcceptInitSynOutput {
        whatami: init_syn_whatami,
        pid: init_syn_pid,
        sn_resolution: init_syn_sn_resolution,
        init_ack_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_session: auth.session,
    };
    Ok(output)
}

struct AcceptInitAckOutput {
    auth_session: AuthenticatedPeerSession,
}
async fn accept_send_init_ack(
    manager: &SessionManager,
    link: &Link,
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
        src: link.get_src(),
        dst: link.get_dst(),
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

    // Use the BlockCipher to enncrypt the cookie
    let serialized = RBuf::from(wbuf).to_vec();
    let mut guard = zasynclock!(manager.prng);
    let encrypted = manager.cipher.encrypt(serialized, &mut *guard);
    drop(guard);
    let cookie = RBuf::from(encrypted);

    let message = SessionMessage::make_init_ack(
        whatami,
        apid,
        sn_resolution,
        cookie,
        input.init_ack_attachment,
    );

    // Send the message on the link
    let _ = link
        .write_session_message(message)
        .await
        .map_err(|e| (e, None))?;

    let output = AcceptInitAckOutput {
        auth_session: input.auth_session,
    };
    Ok(output)
}

struct AcceptOpenSynOutput {
    cookie: Cookie,
    initial_sn: ZInt,
    lease: ZInt,
    open_ack_attachment: Option<Attachment>,
    auth_session: AuthenticatedPeerSession,
}
async fn accept_recv_open_syn(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
    input: AcceptInitAckOutput,
) -> IResult<AcceptOpenSynOutput> {
    // Wait to read an OpenSyn
    let mut messages = link.read_session_message().await.map_err(|e| (e, None))?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single OpenSyn on link {}: {:?}",
            link, messages,
        );
        return Err((
            zerror2!(ZErrorKind::InvalidMessage { descr: e }),
            Some(smsg::close_reason::INVALID),
        ));
    }

    let mut msg = messages.remove(0);
    let (open_syn_lease, open_syn_initial_sn, open_syn_cookie) = match msg.body {
        SessionBody::OpenSyn(OpenSyn {
            lease,
            initial_sn,
            cookie,
        }) => (lease, initial_sn, cookie),
        SessionBody::Close(Close { reason, .. }) => {
            let e = format!(
                "Received a close message (reason {}) instead of an OpenSyn on link: {:?}",
                reason, link,
            );
            return Err((zerror2!(ZErrorKind::InvalidMessage { descr: e }), None));
        }
        _ => {
            let e = format!(
                "Received invalid message instead of an OpenSyn on link {}: {:?}",
                link, msg.body
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
    };

    // Decrypt the cookie with the cyper
    let encrypted = open_syn_cookie.to_vec();
    let decrypted = manager
        .cipher
        .decrypt(encrypted)
        .map_err(|e| (e, Some(smsg::close_reason::INVALID)))?;
    let mut open_syn_cookie = RBuf::from(decrypted);

    // Verify the cookie
    let cookie = match open_syn_cookie.read_cookie() {
        Some(ck) => ck,
        None => {
            let e = format!("Rejecting OpenSyn on link: {}. Invalid cookie.", link,);
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
    };

    // Validate with the peer authenticators
    let open_syn_properties: Vec<Property> = match msg.attachment.take() {
        Some(att) => {
            properties_from_attachment(att).map_err(|e| (e, Some(smsg::close_reason::INVALID)))?
        }
        None => vec![],
    };
    let mut auth = PeerAuthenticatorOutput {
        session: input.auth_session,
        ..Default::default()
    };
    for pa in manager.config.peer_authenticator.iter() {
        let ps = pa
            .handle_open_syn(&auth_link, &open_syn_properties)
            .await
            .map_err(|e| (e, Some(smsg::close_reason::INVALID)))?;
        auth = auth.merge(ps);
    }

    let output = AcceptOpenSynOutput {
        cookie,
        initial_sn: open_syn_initial_sn,
        lease: open_syn_lease,
        open_ack_attachment: attachment_from_properties(&auth.properties).ok(),
        auth_session: auth.session,
    };
    Ok(output)
}

struct AcceptInitSessionOutput {
    session: Session,
    initial_sn: ZInt,
    open_ack_attachment: Option<Attachment>,
}
async fn accept_init_session(
    manager: &SessionManager,
    link: &Link,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptOpenSynOutput,
) -> IResult<AcceptInitSessionOutput> {
    // Initialize the session if it is new
    // NOTE: Keep the lock on the manager.opened and use it to protect concurrent
    //       addition of new sessions and links
    let mut guard = zasynclock!(manager.opened);

    let open_ack_initial_sn = if let Some(opened) = guard.get(&input.cookie.pid) {
        if opened.whatami != input.cookie.whatami
            || opened.sn_resolution != input.cookie.sn_resolution
        {
            let e = format!(
                "Rejecting OpenSyn cookie on link: {}. Invalid sn resolution: {}",
                link, input.cookie.pid
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }
        opened.initial_sn
    } else {
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
    };

    let session = if let Some(session) = manager.get_session(&input.cookie.pid).await {
        // Check if this open is related to a totally new session (i.e. new peer) or to an exsiting one
        // Get the underlying transport
        let transport = session.get_transport().map_err(|e| (e, None))?;

        // Check if we have reached maximum number of links for this session
        if let Some(limit) = manager.config.max_links {
            let links = transport.get_links().await;
            if links.len() >= limit {
                let e = format!(
                    "Rejecting OpenSyn on link: {}. Max links limit reached for peer: {}",
                    link, input.cookie.pid
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidSession { descr: e }),
                    Some(smsg::close_reason::MAX_LINKS),
                ));
            }
        }

        // Check if the sn_resolution is valid (i.e. the same of existing session)
        if input.cookie.sn_resolution != transport.sn_resolution {
            let e = format!(
                "Rejecting OpenSyn on link: {}. Invalid sequence number resolution for peer: {}",
                link, input.cookie.pid
            );
            return Err((
                zerror2!(ZErrorKind::InvalidMessage { descr: e }),
                Some(smsg::close_reason::INVALID),
            ));
        }

        session
    } else {
        // Check if a limit for the maximum number of open sessions is set
        if let Some(limit) = manager.config.max_sessions {
            let num = manager.get_sessions().await.len();
            // Check if we have reached the session limit
            if num >= limit {
                let e = format!(
                    "Rejecting OpenSyn on link: {}. Max sessions limit reached for peer: {}",
                    link, input.cookie.pid
                );
                return Err((
                    zerror2!(ZErrorKind::InvalidSession { descr: e }),
                    Some(smsg::close_reason::MAX_SESSIONS),
                ));
            }
        }

        // Create a new session
        manager
            .new_session(
                &input.cookie.pid,
                &input.cookie.whatami,
                input.cookie.sn_resolution,
                open_ack_initial_sn,
                input.initial_sn,
                input.auth_session.is_local,
            )
            .await
            .map_err(|e| (e, Some(smsg::close_reason::GENERIC)))?
    };

    // Retrieve the session's transport
    let transport = session.get_transport().map_err(|e| (e, None))?;

    // Add the link to the session
    // Compute a suitable keep alive interval based on the lease
    // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
    //       set the actual keep_alive timeout to one fourth of the agreed session lease.
    //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
    //       check which considers a link as failed when no messages are received in 3.5 times the
    //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
    //       session lease.
    let keep_alive = manager.config.keep_alive.min(input.lease / 4);
    let _ = transport
        .add_link(
            link.clone(),
            manager.config.batch_size,
            input.lease,
            keep_alive,
        )
        .await
        .map_err(|e| (e, Some(smsg::close_reason::GENERIC)))?;

    log::debug!(
        "New session link established from {}: {}",
        input.cookie.pid,
        link
    );

    let output = AcceptInitSessionOutput {
        session,
        initial_sn: open_ack_initial_sn,
        open_ack_attachment: input.open_ack_attachment,
    };
    Ok(output)
}

struct AcceptOpenAckOutput {
    session: Session,
}
async fn accept_send_open_ack(
    manager: &SessionManager,
    link: &Link,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptInitSessionOutput,
) -> ZResult<AcceptOpenAckOutput> {
    // Build OpenAck message
    let message = SessionMessage::make_open_ack(
        manager.config.lease,
        input.initial_sn,
        input.open_ack_attachment,
    );

    // Send the message on the link
    let _ = link.write_session_message(message).await?;

    let output = AcceptOpenAckOutput {
        session: input.session,
    };
    Ok(output)
}

async fn accept_finalize_session(
    manager: &SessionManager,
    link: &Link,
    _auth_link: &AuthenticatedPeerLink,
    input: AcceptOpenAckOutput,
) -> ZResult<()> {
    // Retrive the session's transport
    let transport = input.session.get_transport()?;

    // Acquire the lock to avoid concurrent new_session and closing/closed notifications
    let a_guard = transport.get_alive().await;
    if *a_guard {
        // Start the TX loop
        let _ = transport.start_tx(&link).await?;

        // Assign a callback if the session is new
        loop {
            match transport.get_callback().await {
                Some(callback) => {
                    // Notify the session handler there is a new link on this session
                    callback.new_link(link.clone()).await;
                    break;
                }
                None => {
                    // Notify the session handler that there is a new session and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_session() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to.
                    let callback = manager
                        .config
                        .handler
                        .new_session(input.session.clone())
                        .await
                        .map_err(|e| {
                            let e = format!(
                                "Rejecting OpenSyn on link: {}. New session error: {:?}",
                                link, e
                            );
                            zerror2!(ZErrorKind::InvalidSession { descr: e })
                        })?;
                    // Set the callback on the transport
                    transport.set_callback(callback).await;
                }
            }
        }

        // Start the RX loop
        let _ = transport.start_rx(&link).await?;
    }
    drop(a_guard);

    Ok(())
}

async fn accept_link_stages(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
) -> IResult<AcceptInitSessionOutput> {
    let output = accept_recv_init_syn(manager, link, auth_link).await?;
    let output = accept_send_init_ack(manager, link, auth_link, output).await?;
    let output = accept_recv_open_syn(manager, link, auth_link, output).await?;
    accept_init_session(manager, link, auth_link, output).await
}

async fn accept_session_stages(
    manager: &SessionManager,
    link: &Link,
    auth_link: &AuthenticatedPeerLink,
    input: AcceptInitSessionOutput,
) -> ZResult<()> {
    let output = accept_send_open_ack(manager, link, auth_link, input).await?;
    accept_finalize_session(manager, link, auth_link, output).await
}

pub(super) async fn accept_link(
    manager: &SessionManager,
    link: &Link,
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

    let session = output.session.clone();
    let res = accept_session_stages(manager, link, auth_link, output).await;
    if let Err(e) = res {
        let _ = session.close().await;
        return Err(e);
    }

    Ok(())
}
