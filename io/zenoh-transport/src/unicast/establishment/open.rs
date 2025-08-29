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
use std::time::Duration;

use async_trait::async_trait;
use zenoh_buffers::ZSlice;
#[cfg(feature = "transport_auth")]
use zenoh_core::zasynclock;
use zenoh_core::{zcondfeat, zerror};
use zenoh_link::{EndPoint, LinkUnicast};
use zenoh_protocol::{
    core::{Field, Resolution, WhatAmI, ZenohIdProto},
    transport::{
        batch_size, close, BatchSize, Close, InitSyn, OpenSyn, TransportBody, TransportMessage,
        TransportSn,
    },
};
use zenoh_result::ZResult;

#[cfg(feature = "shared-memory")]
use super::ext::shm::AuthSegment;
#[cfg(feature = "shared-memory")]
use crate::shm::TransportShmConfig;
#[cfg(feature = "auth_usrpwd")]
use crate::unicast::establishment::ext::auth::UsrPwdId;
use crate::{
    common::batch::BatchConfig,
    unicast::{
        establishment::{compute_sn, ext, OpenFsm},
        link::{
            LinkUnicastWithOpenAck, TransportLinkUnicast, TransportLinkUnicastConfig,
            TransportLinkUnicastDirection,
        },
        TransportConfigUnicast, TransportUnicast,
    },
    TransportManager,
};

type OpenError = (zenoh_result::Error, Option<u8>);

struct StateTransport {
    batch_size: BatchSize,
    resolution: Resolution,
    ext_qos: ext::qos::StateOpen,
    #[cfg(feature = "transport_multilink")]
    ext_mlink: ext::multilink::StateOpen,
    #[cfg(feature = "shared-memory")]
    ext_shm: ext::shm::StateOpen,
    ext_lowlatency: ext::lowlatency::StateOpen,
    ext_patch: ext::patch::StateOpen,
}

#[cfg(any(feature = "transport_auth", feature = "transport_compression"))]
struct StateLink {
    #[cfg(feature = "transport_auth")]
    ext_auth: ext::auth::StateOpen,
    #[cfg(feature = "transport_compression")]
    ext_compression: ext::compression::StateOpen,
}

struct State {
    transport: StateTransport,
    #[cfg(any(feature = "transport_auth", feature = "transport_compression"))]
    link: StateLink,
}

// InitSyn
struct SendInitSynIn {
    mine_version: u8,
    mine_zid: ZenohIdProto,
    mine_whatami: WhatAmI,
}

// InitAck
struct RecvInitAckOut {
    other_zid: ZenohIdProto,
    other_whatami: WhatAmI,
    other_cookie: ZSlice,
    #[cfg(feature = "shared-memory")]
    ext_shm: Option<AuthSegment>,
}

// OpenSyn
struct SendOpenSynIn {
    mine_zid: ZenohIdProto,
    mine_lease: Duration,
    other_zid: ZenohIdProto,
    other_cookie: ZSlice,
    #[cfg(feature = "shared-memory")]
    ext_shm: Option<AuthSegment>,
}

struct SendOpenSynOut {
    mine_initial_sn: TransportSn,
    #[cfg(feature = "shared-memory")]
    ext_shm: Option<AuthSegment>,
}

// OpenAck
struct RecvOpenAckOut {
    other_lease: Duration,
    other_initial_sn: TransportSn,
}

// FSM
struct OpenLink<'a> {
    ext_qos: ext::qos::QoSFsm<'a>,
    #[cfg(feature = "transport_multilink")]
    ext_mlink: ext::multilink::MultiLinkFsm<'a>,
    #[cfg(feature = "shared-memory")]
    ext_shm: Option<ext::shm::ShmFsm<'a>>,
    #[cfg(feature = "transport_auth")]
    ext_auth: ext::auth::AuthFsm<'a>,
    ext_lowlatency: ext::lowlatency::LowLatencyFsm<'a>,
    #[cfg(feature = "transport_compression")]
    ext_compression: ext::compression::CompressionFsm<'a>,
    ext_patch: ext::patch::PatchFsm<'a>,
}

#[async_trait]
impl<'a, 'b: 'a> OpenFsm for &'a mut OpenLink<'b> {
    type Error = OpenError;

    type SendInitSynIn = (&'a mut TransportLinkUnicast, &'a mut State, SendInitSynIn);
    type SendInitSynOut = ();
    async fn send_init_syn(
        self,
        input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let (link, state, input) = input;

        // Extension QoS
        let (ext_qos, ext_qos_link) = self
            .ext_qos
            .send_init_syn(&state.transport.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        let ext_shm = match self.ext_shm.as_ref() {
            Some(ext) => ext
                .send_init_syn(&state.transport.ext_shm)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None => None,
        };

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_init_syn(&state.link.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_init_syn(&state.transport.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_init_syn(&state.transport.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Compression
        let ext_compression = zcondfeat!(
            "transport_compression",
            self.ext_compression
                .send_init_syn(&state.link.ext_compression)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension Patch
        let ext_patch = self
            .ext_patch
            .send_init_syn(&state.transport.ext_patch)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let msg: TransportMessage = InitSyn {
            version: input.mine_version,
            whatami: input.mine_whatami,
            zid: input.mine_zid,
            batch_size: state.transport.batch_size,
            resolution: state.transport.resolution,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        }
        .into();

        tracing::trace!("Establishment Open InitSyn: {}. Sent: {:?}", link, msg);

        let _ = link
            .send(&msg)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        Ok(())
    }

    type RecvInitAckIn = (&'a mut TransportLinkUnicast, &'a mut State);
    type RecvInitAckOut = RecvInitAckOut;
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (link, state) = input;

        let msg = link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        tracing::trace!("Establishment Open InitAck: {}. Received: {:?}", link, msg);

        let init_ack = match msg.body {
            TransportBody::InitAck(init_ack) => init_ack,
            TransportBody::Close(Close { reason, .. }) => {
                let e = zerror!(
                    "Received a close message (reason {}) in response to an InitSyn on: {}",
                    close::reason_to_str(reason),
                    link,
                );
                match reason {
                    close::reason::MAX_LINKS => tracing::debug!("{}", e),
                    _ => tracing::error!("{}", e),
                }
                return Err((e.into(), None));
            }
            _ => {
                let e = zerror!(
                    "Received an invalid message in response to an InitSyn on {}: {:?}",
                    link,
                    msg.body
                );
                tracing::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };

        // Compute the minimum SN resolution
        state.transport.resolution = {
            let mut res = Resolution::default();

            // Frame SN
            let i_fsn_res = init_ack.resolution.get(Field::FrameSN);
            let m_fsn_res = state.transport.resolution.get(Field::FrameSN);

            if i_fsn_res > m_fsn_res {
                let e = zerror!(
                    "Invalid FrameSN resolution on {}: {:?} > {:?}",
                    link,
                    i_fsn_res,
                    m_fsn_res
                );
                tracing::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
            res.set(Field::FrameSN, i_fsn_res);

            // Request ID
            let i_rid_res = init_ack.resolution.get(Field::RequestID);
            let m_rid_res = state.transport.resolution.get(Field::RequestID);

            if i_rid_res > m_rid_res {
                let e = zerror!(
                    "Invalid RequestID resolution on {}: {:?} > {:?}",
                    link,
                    i_rid_res,
                    m_rid_res
                );
                tracing::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
            res.set(Field::RequestID, i_rid_res);

            res
        };

        // Compute the minimum batch size
        state.transport.batch_size = state.transport.batch_size.min(init_ack.batch_size);

        // Extension QoS
        self.ext_qos
            .recv_init_ack((
                &mut state.transport.ext_qos,
                (init_ack.ext_qos, init_ack.ext_qos_link),
            ))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        let shm_segment = match self.ext_shm.as_ref() {
            Some(ext) => ext
                .recv_init_ack(init_ack.ext_shm)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None => None,
        };

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_init_ack((&mut state.link.ext_auth, init_ack.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_init_ack((&mut state.transport.ext_mlink, init_ack.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_init_ack((&mut state.transport.ext_lowlatency, init_ack.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Compression
        #[cfg(feature = "transport_compression")]
        self.ext_compression
            .recv_init_ack((&mut state.link.ext_compression, init_ack.ext_compression))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Patch
        self.ext_patch
            .recv_init_ack((&mut state.transport.ext_patch, init_ack.ext_patch))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvInitAckOut {
            other_zid: init_ack.zid,
            other_whatami: init_ack.whatami,
            other_cookie: init_ack.cookie,
            #[cfg(feature = "shared-memory")]
            ext_shm: shm_segment,
        };
        Ok(output)
    }

    type SendOpenSynIn = (&'a mut TransportLinkUnicast, &'a mut State, SendOpenSynIn);
    type SendOpenSynOut = SendOpenSynOut;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        let (link, state, input) = input;

        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_open_syn(&state.transport.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        let ext_shm = match self.ext_shm.as_ref() {
            Some(ext_shm) => ext_shm
                .send_open_syn(&input.ext_shm)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None => None,
        };

        // Extension Auth
        let ext_auth = zcondfeat!(
            "transport_auth",
            self.ext_auth
                .send_open_syn(&state.link.ext_auth)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension MultiLink
        let ext_mlink = zcondfeat!(
            "transport_multilink",
            self.ext_mlink
                .send_open_syn(&state.transport.ext_mlink)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Extension LowLatency
        let ext_lowlatency = self
            .ext_lowlatency
            .send_open_syn(&state.transport.ext_lowlatency)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Compression
        let ext_compression = zcondfeat!(
            "transport_compression",
            self.ext_compression
                .send_open_syn(&state.link.ext_compression)
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?,
            None
        );

        // Build and send an OpenSyn message
        let mine_initial_sn =
            compute_sn(input.mine_zid, input.other_zid, state.transport.resolution);
        let msg: TransportMessage = OpenSyn {
            lease: input.mine_lease,
            initial_sn: mine_initial_sn,
            cookie: input.other_cookie,
            ext_qos,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
        }
        .into();

        let _ = link
            .send(&msg)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        tracing::trace!("Establishment Open OpenSyn: {}. Sent: {:?}", link, msg);

        let output = SendOpenSynOut {
            mine_initial_sn,
            #[cfg(feature = "shared-memory")]
            ext_shm: input.ext_shm,
        };
        Ok(output)
    }

    type RecvOpenAckIn = (&'a mut TransportLinkUnicast, &'a mut State);
    type RecvOpenAckOut = RecvOpenAckOut;
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        let (link, state) = input;

        let msg = link
            .recv()
            .await
            .map_err(|e| (e, Some(close::reason::INVALID)))?;

        tracing::trace!("Establishment Open OpenAck: {}. Received: {:?}", link, msg);

        let open_ack = match msg.body {
            TransportBody::OpenAck(open_ack) => open_ack,
            TransportBody::Close(Close { reason, .. }) => {
                let e = zerror!(
                    "Received a close message (reason {}) in response to an OpenSyn on: {:?}",
                    close::reason_to_str(reason),
                    link,
                );
                match reason {
                    close::reason::MAX_LINKS => tracing::debug!("{}", e),
                    _ => tracing::error!("{}", e),
                }
                return Err((e.into(), None));
            }
            _ => {
                let e = zerror!(
                    "Received an invalid message in response to an OpenSyn on {}: {:?}",
                    link,
                    msg.body
                );
                tracing::error!("{}", e);
                return Err((e.into(), Some(close::reason::INVALID)));
            }
        };

        // Extension QoS
        self.ext_qos
            .recv_open_ack((&mut state.transport.ext_qos, open_ack.ext_qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Shm
        #[cfg(feature = "shared-memory")]
        if let Some(ext) = self.ext_shm.as_ref() {
            ext.recv_open_ack((&mut state.transport.ext_shm, open_ack.ext_shm))
                .await
                .map_err(|e| (e, Some(close::reason::GENERIC)))?
        };

        // Extension Auth
        #[cfg(feature = "transport_auth")]
        self.ext_auth
            .recv_open_ack((&mut state.link.ext_auth, open_ack.ext_auth))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension MultiLink
        #[cfg(feature = "transport_multilink")]
        self.ext_mlink
            .recv_open_ack((&mut state.transport.ext_mlink, open_ack.ext_mlink))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension LowLatency
        self.ext_lowlatency
            .recv_open_ack((&mut state.transport.ext_lowlatency, open_ack.ext_lowlatency))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Extension Compression
        #[cfg(feature = "transport_compression")]
        self.ext_compression
            .recv_open_ack((&mut state.link.ext_compression, open_ack.ext_compression))
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
    endpoint: EndPoint,
    link: LinkUnicast,
    manager: &TransportManager,
) -> ZResult<TransportUnicast> {
    let direction = TransportLinkUnicastDirection::Outbound;
    let is_streamed = link.is_streamed();
    let config = TransportLinkUnicastConfig {
        direction,
        batch: BatchConfig {
            mtu: link.get_mtu(),
            is_streamed,
            #[cfg(feature = "transport_compression")]
            is_compression: false, // Perform the exchange Init/Open exchange with no compression
        },
        priorities: None,
        reliability: None,
    };
    let mut link = TransportLinkUnicast::new(link, config);
    let mut fsm = OpenLink {
        ext_qos: ext::qos::QoSFsm::new(),
        #[cfg(feature = "transport_multilink")]
        ext_mlink: manager.state.unicast.multilink.fsm(&manager.prng),
        #[cfg(feature = "shared-memory")]
        ext_shm: manager
            .state
            .shm_context
            .as_ref()
            .map(|ctx| ext::shm::ShmFsm::new(&ctx.auth)),
        #[cfg(feature = "transport_auth")]
        ext_auth: manager.state.unicast.authenticator.fsm(&manager.prng),
        ext_lowlatency: ext::lowlatency::LowLatencyFsm::new(),
        #[cfg(feature = "transport_compression")]
        ext_compression: ext::compression::CompressionFsm::new(),
        ext_patch: ext::patch::PatchFsm::new(),
    };

    // Clippy raises a warning because `batch_size::UNICAST` is currently equal to `BatchSize::MAX`.
    // However, the current code catches the cases where `batch_size::UNICAST` is different from `BatchSize::MAX`.
    #[allow(clippy::unnecessary_min_or_max)]
    let batch_size = manager
        .config
        .batch_size
        .min(link.config.batch.mtu)
        .min(batch_size::UNICAST);

    let mut state = {
        #[cfg(feature = "transport_auth")]
        let mut prng = zasynclock!(manager.prng);

        State {
            transport: StateTransport {
                batch_size,
                resolution: manager.config.resolution,
                ext_qos: ext::qos::StateOpen::new(manager.config.unicast.is_qos, &endpoint)?,
                #[cfg(feature = "transport_multilink")]
                ext_mlink: manager
                    .state
                    .unicast
                    .multilink
                    .open(manager.config.unicast.max_links > 1),
                #[cfg(feature = "shared-memory")]
                ext_shm: ext::shm::StateOpen::new(),
                ext_lowlatency: ext::lowlatency::StateOpen::new(
                    manager.config.unicast.is_lowlatency,
                ),
                ext_patch: ext::patch::StateOpen::new(),
            },
            #[cfg(any(feature = "transport_auth", feature = "transport_compression"))]
            link: StateLink {
                #[cfg(feature = "transport_auth")]
                ext_auth: manager.state.unicast.authenticator.open(&mut *prng),
                #[cfg(feature = "transport_compression")]
                ext_compression: ext::compression::StateOpen::new(
                    manager.config.unicast.is_compression,
                ),
            },
        }
    };

    // Init handshake
    macro_rules! step {
        ($s: expr) => {
            match $s {
                Ok(output) => output,
                Err((e, reason)) => {
                    let _ = link.close(reason).await;
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
    step!(fsm.send_init_syn((&mut link, &mut state, isyn_in)).await);

    let iack_out = step!(fsm.recv_init_ack((&mut link, &mut state)).await);

    // Open handshake
    let osyn_in = SendOpenSynIn {
        mine_zid: manager.config.zid,
        other_zid: iack_out.other_zid,
        mine_lease: manager.config.unicast.lease,
        other_cookie: iack_out.other_cookie,
        #[cfg(feature = "shared-memory")]
        ext_shm: iack_out.ext_shm,
    };
    let osyn_out = step!(fsm.send_open_syn((&mut link, &mut state, osyn_in)).await);

    let oack_out = step!(fsm.recv_open_ack((&mut link, &mut state)).await);

    // Initialize the transport
    let config = TransportConfigUnicast {
        zid: iack_out.other_zid,
        whatami: iack_out.other_whatami,
        sn_resolution: state.transport.resolution.get(Field::FrameSN),
        tx_initial_sn: osyn_out.mine_initial_sn,
        is_qos: state.transport.ext_qos.is_qos(),
        #[cfg(feature = "transport_multilink")]
        multilink: state.transport.ext_mlink.multilink(),
        #[cfg(feature = "shared-memory")]
        shm: match state.transport.ext_shm.negotiated_to_use_shm() {
            true => osyn_out.ext_shm.map(TransportShmConfig::new),
            false => None,
        },
        is_lowlatency: state.transport.ext_lowlatency.is_lowlatency(),
        #[cfg(feature = "auth_usrpwd")]
        auth_id: UsrPwdId(None),
        patch: state.transport.ext_patch.get(),
    };

    let o_config = TransportLinkUnicastConfig {
        direction,
        batch: BatchConfig {
            mtu: state.transport.batch_size,
            is_streamed,
            #[cfg(feature = "transport_compression")]
            is_compression: state.link.ext_compression.is_compression(),
        },
        priorities: state.transport.ext_qos.priorities(),
        reliability: state.transport.ext_qos.reliability(),
    };
    let o_link = link.reconfigure(o_config);
    let s_link = format!("{o_link:?}");
    let o_link = LinkUnicastWithOpenAck::new(o_link, None);
    let transport = manager
        .init_transport_unicast(
            config,
            o_link,
            oack_out.other_initial_sn,
            oack_out.other_lease,
        )
        .await?;

    tracing::debug!(
        "New transport link opened from {} to {}: {}.",
        manager.config.zid,
        iack_out.other_zid,
        s_link,
    );

    Ok(transport)
}
