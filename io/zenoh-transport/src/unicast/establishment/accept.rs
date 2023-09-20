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
#[cfg(feature = "shared-memory")]
use crate::unicast::shared_memory_unicast::Challenge;
use crate::{
    unicast::establishment::{
        close_link, compute_sn, ext, finalize_transport, AcceptFsm, Cookie, InputFinalize,
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
use zenoh_core::{zasynclock, zcondfeat, zerror};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{
    core::{Field, Resolution, WhatAmI, ZenohId},
    transport::{
        batch_size,
        close::{self, Close},
        BatchSize, InitAck, OpenAck, TransportBody, TransportMessage, TransportSn,
    },
};
use zenoh_result::ZResult;

pub(super) type AcceptError = (zenoh_result::Error, Option<u8>);

struct StateZenoh {
    batch_size: BatchSize,
    resolution: Resolution,
}

struct State {
    zenoh: StateZenoh,
    ext_qos: ext::qos::StateAccept,
    #[cfg(feature = "transport_multilink")]
    ext_mlink: ext::multilink::StateAccept,
    #[cfg(feature = "shared-memory")]
    ext_shm: ext::shm::StateAccept,
    #[cfg(feature = "transport_auth")]
    ext_auth: ext::auth::StateAccept,
    ext_lowlatency: ext::lowlatency::StateAccept,
}

// InitSyn
struct RecvInitSynIn {
    mine_version: u8,
}
struct RecvInitSynOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    #[cfg(feature = "shared-memory")]
    ext_shm: Challenge,
}

// InitAck
struct SendInitAckIn {
    mine_version: u8,
    mine_zid: ZenohId,
    mine_whatami: WhatAmI,
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    #[cfg(feature = "shared-memory")]
    ext_shm: Challenge,
}
struct SendInitAckOut {
    cookie_nonce: u64,
}

// OpenSyn
struct RecvOpenSynIn {
    cookie_nonce: u64,
}
struct RecvOpenSynOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    other_lease: Duration,
    other_initial_sn: TransportSn,
}

// OpenAck
struct SendOpenAckIn {
    mine_zid: ZenohId,
    mine_lease: Duration,
    other_zid: ZenohId,
}
struct SendOpenAckOut {
    open_ack: OpenAck,
}

// Fsm
struct AcceptLink<'a> {
    link: &'a LinkUnicast,
    prng: &'a Mutex<PseudoRng>,
    cipher: &'a BlockCipher,
    ext_qos: ext::qos::QoSFsm<'a>,
    #[cfg(feature = "transport_multilink")]
    ext_mlink: ext::multilink::MultiLinkFsm<'a>,
    #[cfg(feature = "shared-memory")]
    ext_shm: ext::shm::ShmFsm<'a>,
    #[cfg(feature = "transport_auth")]
    ext_auth: ext::auth::AuthFsm<'a>,
    ext_lowlatency: ext::lowlatency::LowLatencyFsm<'a>,
}

#[async_trait]
impl<'a> AcceptFsm for AcceptLink<'a> {
    type Error = AcceptError;

    type RecvInitSynIn = (&'a mut State, RecvInitSynIn);
    type RecvInitSynOut = RecvInitSynOut;
    async fn recv_init_syn(
        &self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        let (state, input) = input;

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
        state.zenoh.resolution = {
            let mut res = Resolution::default();

            // Frame SN
            let i_fsn_res = init_syn.resolution.get(Field::FrameSN);
            let m_fsn_res = state.zenoh.resolution.get(Field::FrameSN);
            res.set(Field::FrameSN, i_fsn_res.min(m_fsn_res));

            // Request ID
            let i_rid_res = init_syn.resolution.get(Field::RequestID);
            let m_rid_res = state.zenoh.resolution.get(Field::RequestID);
            res.set(Field::RequestID, i_rid_res.min(m_rid_res));

            res
        };

        // Compute the minimum batch size
        state.zenoh.batch_size = state
            .zenoh
            .batch_size
            .min(init_syn.batch_size)
            .min(batch_size::UNICAST);

        // Extension QoS
        self.ext_qos
            .recv_init_syn((&mut state.ext_qos, init_syn.ext_qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_init_syn((&mut state.ext_lowlatency, init_syn.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        let ext_shm = self
            .ext_shm
            .recv_init_syn((&mut state.ext_shm, init_syn.ext_shm))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_init_syn((&mut state.ext_auth, init_syn.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_init_syn((&mut state.ext_mlink, init_syn.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvInitSynOut {
            other_zid: init_syn.zid,
            other_whatami: init_syn.whatami,
            #[cfg(feature = "shared-memory")]
            ext_shm,
        };
        Ok(output)
    }

    type SendInitAckIn = (State, SendInitAckIn);
    type SendInitAckOut = SendInitAckOut;
    async fn send_init_ack(
        &self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        #[allow(unused_mut)] // Required for "shared-memory" feature
        let (mut state, input) = input;

        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_init_ack(&state.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_init_ack(&state.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        let ext_shm = zcondfeat!(
            "shared-memory",
            self.ext_shm
                .send_init_ack((&mut state.ext_shm, input.ext_shm))
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_init_ack(&state.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_init_ack(&state.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Create the cookie
        let cookie_nonce: u64 = zasynclock!(self.prng).gen();
        let cookie = Cookie {
            zid: input.other_zid,
            whatami: input.other_whatami,
            resolution: state.zenoh.resolution,
            batch_size: state.zenoh.batch_size,
            nonce: cookie_nonce,
            ext_qos: state.ext_qos,
            #[cfg(feature = "transport_multilink")]
            ext_mlink: state.ext_mlink,
            #[cfg(feature = "shared-memory")]
            ext_shm: state.ext_shm,
            #[cfg(feature = "transport_auth")]
            ext_auth: state.ext_auth,
            ext_lowlatency: state.ext_lowlatency,
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
            resolution: state.zenoh.resolution,
            batch_size: state.zenoh.batch_size,
            cookie,
            ext_qos,
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
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

    type RecvOpenSynIn = RecvOpenSynIn;
    type RecvOpenSynOut = (State, RecvOpenSynOut);
    async fn recv_open_syn(
        &self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
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
        let cookie: Cookie = {
            let mut codec = Zenoh080Cookie {
                prng: &mut *zasynclock!(self.prng),
                cipher: self.cipher,
                codec: Zenoh080::new(),
            };
            let mut reader = encrypted.reader();
            codec.read(&mut reader).map_err(|_| {
                (
                    zerror!("Decoding cookie failed").into(),
                    Some(close::reason::INVALID),
                )
            })
        }?;

        // Verify that the cookie is the one we sent
        if input.cookie_nonce != cookie.nonce {
            let e = zerror!("Rejecting OpenSyn on: {}. Unkwown cookie.", self.link);
            return Err((e.into(), Some(close::reason::INVALID)));
        }

        // Rebuild the state from the cookie
        let mut state = State {
            zenoh: StateZenoh {
                batch_size: cookie.batch_size,
                resolution: cookie.resolution,
            },
            ext_qos: cookie.ext_qos,
            #[cfg(feature = "transport_multilink")]
            ext_mlink: cookie.ext_mlink,
            #[cfg(feature = "shared-memory")]
            ext_shm: cookie.ext_shm,
            #[cfg(feature = "transport_auth")]
            ext_auth: cookie.ext_auth,
            ext_lowlatency: cookie.ext_lowlatency,
        };

        // Extension QoS
        self.ext_qos
            .recv_open_syn((&mut state.ext_qos, open_syn.ext_qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_open_syn((&mut state.ext_lowlatency, open_syn.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        self.ext_shm
            .recv_open_syn((&mut state.ext_shm, open_syn.ext_shm))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_open_syn((&mut state.ext_auth, open_syn.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_open_syn((&mut state.ext_mlink, open_syn.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvOpenSynOut {
            other_zid: cookie.zid,
            other_whatami: cookie.whatami,
            other_lease: open_syn.lease,
            other_initial_sn: open_syn.initial_sn,
        };
        Ok((state, output))
    }

    type SendOpenAckIn = (&'a mut State, SendOpenAckIn);
    type SendOpenAckOut = SendOpenAckOut;
    async fn send_open_ack(
        &self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        let (state, input) = input;

        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_open_ack(&state.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_open_ack(&state.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        let ext_shm = zcondfeat!(
            "shared-memory",
            self.ext_shm
                .send_open_ack(&mut state.ext_shm)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_open_ack(&state.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_open_ack(&state.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Build OpenAck message
        let mine_initial_sn = compute_sn(input.mine_zid, input.other_zid, state.zenoh.resolution);
        let open_ack = OpenAck {
            lease: input.mine_lease,
            initial_sn: mine_initial_sn,
            ext_qos,
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
        };

        // Do not send the OpenAck right now since we might still incur in MAX_LINKS error

        let output = SendOpenAckOut { open_ack };
        Ok(output)
    }
}

pub(crate) async fn accept_link(link: &LinkUnicast, manager: &TransportManager) -> ZResult<()> {
    let fsm = AcceptLink {
        link,
        prng: &manager.prng,
        cipher: &manager.cipher,
        ext_qos: ext::qos::QoSFsm::new(),
        #[cfg(feature = "shared-memory")]
        ext_shm: ext::shm::ShmFsm::new(&manager.state.unicast.shm),
        #[cfg(feature = "transport_multilink")]
        ext_mlink: manager.state.unicast.multilink.fsm(&manager.prng),
        #[cfg(feature = "transport_auth")]
        ext_auth: manager.state.unicast.authenticator.fsm(&manager.prng),
        ext_lowlatency: ext::lowlatency::LowLatencyFsm::new(),
    };

    // Init handshake
    macro_rules! step {
        ($res:expr) => {
            match $res {
                Ok(output) => output,
                Err((e, reason)) => {
                    log::debug!("{}", e);
                    close_link(link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    let iack_out = {
        let mut state = State {
            zenoh: StateZenoh {
                batch_size: manager.config.batch_size,
                resolution: manager.config.resolution,
            },
            ext_qos: ext::qos::StateAccept::new(manager.config.unicast.is_qos),
            ext_lowlatency: ext::lowlatency::StateAccept::new(manager.config.unicast.is_lowlatency),
            #[cfg(feature = "transport_multilink")]
            ext_mlink: manager
                .state
                .unicast
                .multilink
                .accept(manager.config.unicast.max_links > 1),
            #[cfg(feature = "shared-memory")]
            ext_shm: ext::shm::StateAccept::new(manager.config.unicast.is_shm),
            #[cfg(feature = "transport_auth")]
            ext_auth: manager
                .state
                .unicast
                .authenticator
                .accept(&mut *zasynclock!(manager.prng)),
        };

        // Let's scope the Init phase in such a way memory is freed by Rust
        // after having sent the InitAck. The state will be recovered
        // from the Cookie received in the OpenSyn.
        let isyn_in = RecvInitSynIn {
            mine_version: manager.config.version,
        };
        let isyn_out = step!(fsm.recv_init_syn((&mut state, isyn_in)).await);

        let iack_in = SendInitAckIn {
            mine_version: manager.config.version,
            mine_zid: manager.config.zid,
            mine_whatami: manager.config.whatami,
            other_zid: isyn_out.other_zid,
            other_whatami: isyn_out.other_whatami,
            #[cfg(feature = "shared-memory")]
            ext_shm: isyn_out.ext_shm,
        };
        step!(fsm.send_init_ack((state, iack_in)).await)
    };

    // Open handshake
    let osyn_in = RecvOpenSynIn {
        cookie_nonce: iack_out.cookie_nonce,
    };
    let (mut state, osyn_out) = step!(fsm.recv_open_syn(osyn_in).await);

    // Create the OpenAck but not send it yet
    let oack_in = SendOpenAckIn {
        mine_zid: manager.config.zid,
        mine_lease: manager.config.unicast.lease,
        other_zid: osyn_out.other_zid,
    };
    let oack_out = step!(fsm.send_open_ack((&mut state, oack_in)).await);

    // Initialize the transport
    let config = TransportConfigUnicast {
        zid: osyn_out.other_zid,
        whatami: osyn_out.other_whatami,
        sn_resolution: state.zenoh.resolution.get(Field::FrameSN),
        tx_initial_sn: oack_out.open_ack.initial_sn,
        is_qos: state.ext_qos.is_qos(),
        #[cfg(feature = "transport_multilink")]
        multilink: state.ext_mlink.multilink(),
        #[cfg(feature = "shared-memory")]
        is_shm: state.ext_shm.is_shm(),
        is_lowlatency: state.ext_lowlatency.is_lowlatency(),
    };

    let transport = step!(
        manager
            .init_transport_unicast(config, link.clone(), LinkUnicastDirection::Inbound)
            .await
    );

    // Send the open_ack on the link
    step!(link
        .send(&oack_out.open_ack.into())
        .await
        .map_err(|e| (e, Some(close::reason::GENERIC))));

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .sync(osyn_out.other_initial_sn)
    .await;

    // Finalize the transport
    let input = InputFinalize {
        transport: transport.clone(),
        other_lease: osyn_out.other_lease,
        agreed_batch_size: state.zenoh.batch_size,
    };
    step!(finalize_transport(link, manager, input)
        .await
        .map_err(|e| (e, Some(close::reason::INVALID))));

    log::debug!(
        "New transport link accepted from {} to {}: {}",
        osyn_out.other_zid,
        manager.config.zid,
        link
    );

    Ok(())
}
