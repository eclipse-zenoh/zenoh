//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::{
    unicast::establishment::{
        close_link, compute_sn, finalize_transport, AcceptFsm, Cookie, InputFinalize,
        Zenoh080Cookie,
    },
    TransportConfigUnicast, TransportManager,
};
use async_std::sync::Mutex;
use async_trait::async_trait;
use rand::Rng;
use std::time::Duration;
use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZSlice};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{zasynclock, zerror};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{
    core::{Field, Resolution, WhatAmI, ZInt, ZenohId},
    transport::{
        close::{self, Close},
        InitAck, OpenAck, TransportBody, TransportMessage,
    },
};
use zenoh_result::ZResult;

pub(super) type AcceptError = (zenoh_result::Error, Option<u8>);

// InitSyn
struct RecvInitSynIn {
    mine_version: u8,
    mine_resolution: Resolution,
    mine_batch_size: u16,
}
struct RecvInitSynOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    agreed_resolution: Resolution,
    agreed_batch_size: u16,
}

// InitAck
struct SendInitAckIn {
    mine_version: u8,
    mine_zid: ZenohId,
    mine_whatami: WhatAmI,
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    agreed_resolution: Resolution,
    agreed_batch_size: u16,
}
struct SendInitAckOut {
    cookie_nonce: ZInt,
}

// OpenSyn
struct RecvOpenSynIn {
    cookie_nonce: ZInt,
}
struct RecvOpenSynOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    other_lease: Duration,
    other_initial_sn: ZInt,
    agreed_resolution: Resolution,
    agreed_batch_size: u16,
}

// OpenAck
struct SendOpenAckIn {
    mine_lease: Duration,
    mine_initial_sn: ZInt,
}
struct SendOpenAckOut {}

// Fsm
struct AcceptLink<'a> {
    link: &'a LinkUnicast,
    prng: &'a Mutex<PseudoRng>,
    cipher: &'a BlockCipher,
}

#[async_trait]
impl<'a> AcceptFsm for AcceptLink<'a> {
    type Error = AcceptError;

    type InitSynIn = RecvInitSynIn;
    type InitSynOut = RecvInitSynOut;
    async fn recv_init_syn(&self, input: Self::InitSynIn) -> Result<Self::InitSynOut, Self::Error> {
        let msg = self
            .link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        let init_syn = match msg.body {
            TransportBody::InitSyn(init_syn) => init_syn,
            _ => {
                let e = zerror!(
                    "Received invalid message instead of an InitSyn on {}: {:?}",
                    self.link,
                    msg.body
                );
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };

        // Check if the version is supported
        if init_syn.version != input.mine_version {
            let e = zerror!(
                "Rejecting InitSyn on {} because of unsupported Zenoh version from peer: {}",
                self.link,
                init_syn.zid
            );
            return Err((e.into(), Some(close::reason::INVALID)));
        }

        // Compute the minimum SN resolution
        let agreed_resolution = {
            let mut res = Resolution::default();

            // Frame SN
            let i_fsn_res = init_syn.resolution.get(Field::FrameSN);
            let m_fsn_res = input.mine_resolution.get(Field::FrameSN);
            res.set(Field::FrameSN, i_fsn_res.min(m_fsn_res));

            // Request ID
            let i_rid_res = input.mine_resolution.get(Field::RequestID);
            let m_rid_res = input.mine_resolution.get(Field::RequestID);
            res.set(Field::RequestID, i_rid_res.min(m_rid_res));

            res
        };

        // Compute the minimum batch size
        let agreed_batch_size = input.mine_batch_size.min(input.mine_batch_size);

        let output = RecvInitSynOut {
            other_whatami: init_syn.whatami,
            other_zid: init_syn.zid,
            agreed_resolution,
            agreed_batch_size,
        };
        Ok(output)
    }

    type InitAckIn = SendInitAckIn;
    type InitAckOut = SendInitAckOut;
    async fn send_init_ack(&self, input: Self::InitAckIn) -> Result<Self::InitAckOut, Self::Error> {
        // Create the cookie
        let cookie_nonce: ZInt = zasynclock!(self.prng).gen();
        let cookie = Cookie {
            whatami: input.other_whatami,
            zid: input.other_zid,
            resolution: input.agreed_resolution,
            batch_size: input.agreed_batch_size,
            nonce: cookie_nonce,
            is_qos: false, // @TODO
                           // properties: EstablishmentProperties::new(),
        };

        let mut encrypted = vec![];
        let mut writer = encrypted.writer();
        let mut codec = Zenoh080Cookie {
            prng: &mut *zasynclock!(self.prng),
            cipher: self.cipher,
            codec: Zenoh080::new(),
        };
        codec.write(&mut writer, &cookie).map_err(|_| {
            (
                zerror!("Encoding cookie failed").into(),
                Some(close::reason::INVALID),
            )
        })?;
        let cookie: ZSlice = encrypted.into();

        // Send the message on the link
        let message: TransportMessage = InitAck {
            version: input.mine_version,
            whatami: input.mine_whatami,
            zid: input.mine_zid,
            resolution: input.agreed_resolution,
            batch_size: input.agreed_batch_size,
            cookie,
            qos: None,  // @TODO
            shm: None,  // @TODO
            auth: None, // @TODO
        }
        .into();

        let _ = self
            .link
            .send(&message)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = SendInitAckOut { cookie_nonce };
        Ok(output)
    }

    type OpenSynIn = RecvOpenSynIn;
    type OpenSynOut = RecvOpenSynOut;
    async fn recv_open_syn(&self, input: Self::OpenSynIn) -> Result<Self::OpenSynOut, Self::Error> {
        let msg = self
            .link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        let open_syn = match msg.body {
            TransportBody::OpenSyn(open_syn) => open_syn,
            TransportBody::Close(Close { reason, .. }) => {
                let e = zerror!(
                    "Received a close message (reason {}) instead of an OpenSyn on: {:?}",
                    close::reason_to_str(reason),
                    self.link,
                );
                match reason {
                    close::reason::MAX_LINKS => log::debug!("{}", e),
                    _ => log::error!("{}", e),
                }
                return Err((e.into(), None));
            }
            _ => {
                let e = zerror!(
                    "Received invalid message instead of an OpenSyn on {}: {:?}",
                    self.link,
                    msg.body
                );
                log::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };
        let encrypted = open_syn.cookie.to_vec();

        // Decrypt the cookie with the cipher
        let mut reader = encrypted.reader();
        let mut codec = Zenoh080Cookie {
            prng: &mut *zasynclock!(self.prng),
            cipher: self.cipher,
            codec: Zenoh080::new(),
        };
        let cookie: Cookie = codec.read(&mut reader).map_err(|_| {
            (
                zerror!("Decoding cookie failed").into(),
                Some(close::reason::INVALID),
            )
        })?;

        // Verify that the cookie is the one we sent
        if input.cookie_nonce != cookie.nonce {
            let e = zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", self.link);
            return Err((e.into(), Some(close::reason::INVALID)));
        }

        let output = RecvOpenSynOut {
            other_zid: cookie.zid,
            other_whatami: cookie.whatami,
            other_lease: open_syn.lease,
            other_initial_sn: open_syn.initial_sn,
            agreed_resolution: cookie.resolution,
            agreed_batch_size: cookie.batch_size,
        };
        Ok(output)
    }

    type OpenAckIn = SendOpenAckIn;
    type OpenAckOut = SendOpenAckOut;
    async fn send_open_ack(&self, input: Self::OpenAckIn) -> Result<Self::OpenAckOut, Self::Error> {
        // Build OpenAck message
        let message: TransportMessage = OpenAck {
            lease: input.mine_lease,
            initial_sn: input.mine_initial_sn,
            auth: None, // @TODO
        }
        .into();

        // Send the message on the link
        let _ = self
            .link
            .send(&message)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = SendOpenAckOut {};
        Ok(output)
    }
}

pub(crate) async fn accept_link(link: &LinkUnicast, manager: &TransportManager) -> ZResult<()> {
    let fsm = AcceptLink {
        link,
        prng: &manager.prng,
        cipher: &manager.cipher,
    };

    // Init handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    log::error!("{}", e);
                    close_link(link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    let iack_out = {
        // Let's scope the Init phase in such a way memory is freed by Rust
        // after having sent the InitAck. The state will be recovered
        // from the Cookie received in the OpenSyn.
        let isyn_in = RecvInitSynIn {
            mine_version: manager.config.version,
            mine_resolution: manager.config.resolution,
            mine_batch_size: manager.config.batch_size,
        };
        let isyn_out = step!(fsm.recv_init_syn(isyn_in).await);

        let iack_in = SendInitAckIn {
            mine_version: manager.config.version,
            mine_zid: manager.config.zid,
            mine_whatami: manager.config.whatami,
            other_zid: isyn_out.other_zid,
            other_whatami: isyn_out.other_whatami,
            agreed_resolution: isyn_out.agreed_resolution,
            agreed_batch_size: isyn_out.agreed_batch_size,
        };
        step!(fsm.send_init_ack(iack_in).await)
    };

    // Open handshake
    let osyn_in = RecvOpenSynIn {
        cookie_nonce: iack_out.cookie_nonce,
    };
    let osyn_out = step!(fsm.recv_open_syn(osyn_in).await);

    // Initialize the transport before sending the ack to verify that the transport is ok

    // 1) Create a random yet deterministic initial_sn.
    let mine_initial_sn = compute_sn(
        manager.config.zid,
        osyn_out.other_zid,
        osyn_out.agreed_resolution,
    );

    // 2) Initialize the transport.
    let config = TransportConfigUnicast {
        peer: osyn_out.other_zid,
        whatami: osyn_out.other_whatami,
        sn_resolution: osyn_out.agreed_resolution.get(Field::FrameSN).mask(),
        tx_initial_sn: mine_initial_sn,
        is_qos: false, // @TODO
        is_shm: false, // @TODO
    };
    let transport = step!(manager
        .init_transport_unicast(config)
        .await
        .map_err(|e| (e, Some(close::reason::INVALID))));

    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    match reason {
                        Some(close::reason::MAX_LINKS) => log::debug!("{}", e),
                        _ => log::error!("{}", e),
                    }
                    if let Ok(ll) = transport.get_links() {
                        if ll.is_empty() {
                            let _ = manager.del_transport_unicast(&osyn_out.other_zid).await;
                        }
                    }
                    close_link(link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    // 3) Add the link to the transport
    step!(step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .add_link(link.clone(), LinkUnicastDirection::Inbound)
    .map_err(|e| (e, Some(close::reason::MAX_LINKS))));

    // 4) Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .sync(osyn_out.other_initial_sn)
    .await;

    // Send the OpenAck
    let oack_in = SendOpenAckIn {
        mine_lease: manager.config.unicast.lease,
        mine_initial_sn,
    };
    let _ = step!(fsm.send_open_ack(oack_in).await);

    // Finalize the transport
    let input = InputFinalize {
        transport: transport.clone(),
        other_lease: osyn_out.other_lease,
        agreed_batch_size: osyn_out.agreed_batch_size,
    };
    step!(finalize_transport(link, manager, input)
        .await
        .map_err(|e| (e, Some(close::reason::INVALID))));

    log::debug!(
        "New transport link established from {}: {}",
        osyn_out.other_zid,
        link
    );

    Ok(())
}
