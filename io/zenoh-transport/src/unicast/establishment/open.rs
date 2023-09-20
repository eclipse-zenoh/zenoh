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
        close_link, compute_sn, ext, finalize_transport, InputFinalize, OpenFsm,
    },
    TransportConfigUnicast, TransportManager, TransportUnicast,
};
use async_trait::async_trait;
use std::time::Duration;
use zenoh_buffers::ZSlice;
#[cfg(feature = "transport_auth")]
use zenoh_core::zasynclock;
use zenoh_core::{zcondfeat, zerror};
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{
    core::{Field, Resolution, WhatAmI, ZenohId},
    transport::{
        batch_size, close, BatchSize, Close, InitSyn, OpenSyn, TransportBody, TransportMessage,
        TransportSn,
    },
};
use zenoh_result::ZResult;

type OpenError = (zenoh_result::Error, Option<u8>);

struct StateZenoh {
    batch_size: BatchSize,
    resolution: Resolution,
}

struct State {
    zenoh: StateZenoh,
    ext_qos: ext::qos::StateOpen,
    #[cfg(feature = "transport_multilink")]
    ext_mlink: ext::multilink::StateOpen,
    #[cfg(feature = "shared-memory")]
    ext_shm: ext::shm::StateOpen,
    #[cfg(feature = "transport_auth")]
    ext_auth: ext::auth::StateOpen,
    ext_lowlatency: ext::lowlatency::StateOpen,
}

// InitSyn
struct SendInitSynIn {
    mine_version: u8,
    mine_zid: ZenohId,
    mine_whatami: WhatAmI,
}

// InitAck
struct RecvInitAckOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    other_cookie: ZSlice,
    #[cfg(feature = "shared-memory")]
    ext_shm: Challenge,
}

// OpenSyn
struct SendOpenSynIn {
    mine_zid: ZenohId,
    mine_lease: Duration,
    other_zid: ZenohId,
    other_cookie: ZSlice,
    #[cfg(feature = "shared-memory")]
    ext_shm: Challenge,
}

struct SendOpenSynOut {
    mine_initial_sn: TransportSn,
}

// OpenAck
struct RecvOpenAckOut {
    other_lease: Duration,
    other_initial_sn: TransportSn,
}

// FSM
struct OpenLink<'a> {
    link: &'a LinkUnicast,
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
impl<'a> OpenFsm for OpenLink<'a> {
    type Error = OpenError;

    type SendInitSynIn = (&'a mut State, SendInitSynIn);
    type SendInitSynOut = ();
    async fn send_init_syn(
        &self,
        input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let (state, input) = input;

        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_init_syn(&state.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_init_syn(&state.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        let ext_shm = zcondfeat!(
            "shared-memory",
            self.ext_shm
                .send_init_syn(&state.ext_shm)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_init_syn(&state.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_init_syn(&state.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        let msg: TransportMessage = InitSyn {
            version: input.mine_version,
            whatami: input.mine_whatami,
            zid: input.mine_zid,
            batch_size: state.zenoh.batch_size,
            resolution: state.zenoh.resolution,
            ext_qos,
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
        }
        .into();

        let _ = self
            .link
            .send(&msg)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        Ok(())
    }

    type RecvInitAckIn = &'a mut State;
    type RecvInitAckOut = RecvInitAckOut;
    async fn recv_init_ack(
        &self,
        state: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let msg = self
            .link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        let init_ack = match msg.body {
            TransportBody::InitAck(init_ack) => init_ack,
            TransportBody::Close(Close { reason, .. }) => {
                let e = zerror!(
                    "Received a close message (reason {}) in response to an InitSyn on: {}",
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
                    "Received an invalid message in response to an InitSyn on {}: {:?}",
                    self.link,
                    msg.body
                );
                log::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };

        // Compute the minimum SN resolution
        state.zenoh.resolution = {
            let mut res = Resolution::default();

            // Frame SN
            let i_fsn_res = init_ack.resolution.get(Field::FrameSN);
            let m_fsn_res = state.zenoh.resolution.get(Field::FrameSN);

            if i_fsn_res > m_fsn_res {
                let e = zerror!(
                    "Invalid FrameSN resolution on {}: {:?} > {:?}",
                    self.link,
                    i_fsn_res,
                    m_fsn_res
                );
                log::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
            res.set(Field::FrameSN, i_fsn_res);

            // Request ID
            let i_rid_res = init_ack.resolution.get(Field::RequestID);
            let m_rid_res = state.zenoh.resolution.get(Field::RequestID);

            if i_rid_res > m_rid_res {
                let e = zerror!(
                    "Invalid RequestID resolution on {}: {:?} > {:?}",
                    self.link,
                    i_rid_res,
                    m_rid_res
                );
                log::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
            res.set(Field::RequestID, i_rid_res);

            res
        };

        // Compute the minimum batch size
        state.zenoh.batch_size = state.zenoh.batch_size.min(init_ack.batch_size);

        // Extension QoS
        self.ext_qos
            .recv_init_ack((&mut state.ext_qos, init_ack.ext_qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_init_ack((&mut state.ext_lowlatency, init_ack.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        let shm_challenge = self
            .ext_shm
            .recv_init_ack((&mut state.ext_shm, init_ack.ext_shm))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_init_ack((&mut state.ext_auth, init_ack.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_init_ack((&mut state.ext_mlink, init_ack.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvInitAckOut {
            other_zid: init_ack.zid,
            other_whatami: init_ack.whatami,
            other_cookie: init_ack.cookie,
            #[cfg(feature = "shared-memory")]
            ext_shm: shm_challenge,
        };
        Ok(output)
    }

    type SendOpenSynIn = (&'a mut State, SendOpenSynIn);
    type SendOpenSynOut = SendOpenSynOut;
    async fn send_open_syn(
        &self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        let (state, input) = input;

        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_open_syn(&state.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_open_syn(&state.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        let ext_shm = zcondfeat!(
            "shared-memory",
            self.ext_shm
                .send_open_syn((&state.ext_shm, input.ext_shm))
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_open_syn(&state.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_open_syn(&state.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Build and send an OpenSyn message
        let mine_initial_sn = compute_sn(input.mine_zid, input.other_zid, state.zenoh.resolution);
        let message: TransportMessage = OpenSyn {
            lease: input.mine_lease,
            initial_sn: mine_initial_sn,
            cookie: input.other_cookie,
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

        let output = SendOpenSynOut { mine_initial_sn };
        Ok(output)
    }

    type RecvOpenAckIn = &'a mut State;
    type RecvOpenAckOut = RecvOpenAckOut;
    async fn recv_open_ack(
        &self,
        state: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        let msg = self
            .link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        let open_ack = match msg.body {
            TransportBody::OpenAck(open_ack) => open_ack,
            TransportBody::Close(Close { reason, .. }) => {
                let e = zerror!(
                    "Received a close message (reason {}) in response to an OpenSyn on: {:?}",
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
                    "Received an invalid message in response to an OpenSyn on {}: {:?}",
                    self.link,
                    msg.body
                );
                log::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };

        // Extension QoS
        self.ext_qos
            .recv_open_ack((&mut state.ext_qos, open_ack.ext_qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_open_ack((&mut state.ext_lowlatency, open_ack.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        self.ext_shm
            .recv_open_ack((&mut state.ext_shm, open_ack.ext_shm))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_open_ack((&mut state.ext_auth, open_ack.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_open_ack((&mut state.ext_mlink, open_ack.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvOpenAckOut {
            other_initial_sn: open_ack.initial_sn,
            other_lease: open_ack.lease,
        };
        Ok(output)
    }
}

pub(crate) async fn open_link(
    link: &LinkUnicast,
    manager: &TransportManager,
) -> ZResult<TransportUnicast> {
    let fsm = OpenLink {
        link,
        ext_qos: ext::qos::QoSFsm::new(),
        #[cfg(feature = "transport_multilink")]
        ext_mlink: manager.state.unicast.multilink.fsm(&manager.prng),
        #[cfg(feature = "shared-memory")]
        ext_shm: ext::shm::ShmFsm::new(&manager.state.unicast.shm),
        #[cfg(feature = "transport_auth")]
        ext_auth: manager.state.unicast.authenticator.fsm(&manager.prng),
        ext_lowlatency: ext::lowlatency::LowLatencyFsm::new(),
    };

    let mut state = State {
        zenoh: StateZenoh {
            batch_size: manager.config.batch_size.min(batch_size::UNICAST),
            resolution: manager.config.resolution,
        },
        ext_qos: ext::qos::StateOpen::new(manager.config.unicast.is_qos),
        #[cfg(feature = "transport_multilink")]
        ext_mlink: manager
            .state
            .unicast
            .multilink
            .open(manager.config.unicast.max_links > 1),
        #[cfg(feature = "shared-memory")]
        ext_shm: ext::shm::StateOpen::new(manager.config.unicast.is_shm),
        #[cfg(feature = "transport_auth")]
        ext_auth: manager
            .state
            .unicast
            .authenticator
            .open(&mut *zasynclock!(manager.prng)),
        ext_lowlatency: ext::lowlatency::StateOpen::new(manager.config.unicast.is_lowlatency),
    };

    // Init handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    close_link(link, reason).await;
                    return Err(e);
                }
            }
        };
    }

    let isyn_in = SendInitSynIn {
        mine_version: manager.config.version,
        mine_zid: manager.config.zid,
        mine_whatami: manager.config.whatami,
    };
    step!(fsm.send_init_syn((&mut state, isyn_in)).await);

    let iack_out = step!(fsm.recv_init_ack(&mut state).await);

    // Open handshake
    let osyn_in = SendOpenSynIn {
        mine_zid: manager.config.zid,
        other_zid: iack_out.other_zid,
        mine_lease: manager.config.unicast.lease,
        other_cookie: iack_out.other_cookie,
        #[cfg(feature = "shared-memory")]
        ext_shm: iack_out.ext_shm,
    };
    let osyn_out = step!(fsm.send_open_syn((&mut state, osyn_in)).await);

    let oack_out = step!(fsm.recv_open_ack(&mut state).await);

    // Initialize the transport
    let config = TransportConfigUnicast {
        zid: iack_out.other_zid,
        whatami: iack_out.other_whatami,
        sn_resolution: state.zenoh.resolution.get(Field::FrameSN),
        tx_initial_sn: osyn_out.mine_initial_sn,
        is_qos: state.ext_qos.is_qos(),
        #[cfg(feature = "transport_multilink")]
        multilink: state.ext_mlink.multilink(),
        #[cfg(feature = "shared-memory")]
        is_shm: state.ext_shm.is_shm(),
        is_lowlatency: state.ext_lowlatency.is_lowlatency(),
    };

    let transport = step!(
        manager
            .init_transport_unicast(config, link.clone(), LinkUnicastDirection::Outbound)
            .await
    );

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .sync(oack_out.other_initial_sn)
    .await;

    let output = InputFinalize {
        transport,
        other_lease: oack_out.other_lease,
        agreed_batch_size: state.zenoh.batch_size,
    };
    let transport = output.transport.clone();
    let res = finalize_transport(link, manager, output).await;
    if let Err(e) = res {
        let _ = transport.close().await;
        return Err(e);
    }

    log::debug!(
        "New transport link opened from {} to {}: {}",
        manager.config.zid,
        iack_out.other_zid,
        link
    );

    Ok(transport)
}
