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
use crate::unicast::establishment::{
    close_link, compute_sn, ext, finalize_transport, InputFinalize, OpenFsm,
};
use crate::{TransportConfigUnicast, TransportManager, TransportUnicast};
use async_trait::async_trait;
use std::time::Duration;
use zenoh_buffers::ZSlice;
use zenoh_config::{WhatAmI, ZenohId};
use zenoh_core::zerror;
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::core::{Field, Resolution, ZInt};
use zenoh_protocol::transport::{close, Close, InitSyn, OpenSyn, TransportBody, TransportMessage};
use zenoh_result::ZResult;

type OpenError = (zenoh_result::Error, Option<u8>);

// InitSyn
struct SendInitSynIn {
    mine_version: u8,
    mine_zid: ZenohId,
    mine_whatami: WhatAmI,
    mine_resolution: Resolution,
    mine_batch_size: u16,
    ext_qos: ext::qos::State,
}

struct SendInitSynOut {
    mine_batch_size: u16,
    mine_resolution: Resolution,
    ext_qos: ext::qos::State,
}

// InitAck
struct RecvInitAckIn {
    mine_batch_size: u16,
    mine_resolution: Resolution,
    ext_qos: ext::qos::State,
}

struct RecvInitAckOut {
    other_zid: ZenohId,
    other_whatami: WhatAmI,
    other_cookie: ZSlice,
    agreed_resolution: Resolution,
    agreed_batch_size: u16,
    ext_qos: ext::qos::State,
}

// OpenSyn
struct SendOpenSynIn {
    mine_zid: ZenohId,
    mine_lease: Duration,
    other_zid: ZenohId,
    other_cookie: ZSlice,
    agreed_resolution: Resolution,
    ext_qos: ext::qos::State,
}

struct SendOpenSynOut {
    mine_initial_sn: ZInt,
    ext_qos: ext::qos::State,
}

// OpenAck
struct RecvOpenAckIn {
    ext_qos: ext::qos::State,
}

struct RecvOpenAckOut {
    other_lease: Duration,
    other_initial_sn: ZInt,
    ext_qos: ext::qos::State,
}

// FSM
struct OpenLink<'a> {
    link: &'a LinkUnicast,
    ext_qos: ext::qos::QoS,
}

#[async_trait]
impl<'a> OpenFsm for OpenLink<'a> {
    type Error = OpenError;

    type InitSynIn = SendInitSynIn;
    type InitSynOut = SendInitSynOut;
    async fn send_init_syn(&self, input: Self::InitSynIn) -> Result<Self::InitSynOut, Self::Error> {
        // Extension QoS
        let (qos_state, qos_mine_ext) = self
            .ext_qos
            .send_init_syn(input.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let msg: TransportMessage = InitSyn {
            version: input.mine_version,
            whatami: input.mine_whatami,
            zid: input.mine_zid,
            batch_size: input.mine_batch_size,
            resolution: input.mine_resolution,
            qos: qos_mine_ext,
            shm: None,  // @TODO
            auth: None, // @TODO
        }
        .into();

        let _ = self
            .link
            .send(&msg)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = SendInitSynOut {
            mine_batch_size: input.mine_batch_size,
            mine_resolution: input.mine_resolution,
            ext_qos: qos_state,
        };

        Ok(output)
    }

    type InitAckIn = RecvInitAckIn;
    type InitAckOut = RecvInitAckOut;
    async fn recv_init_ack(&self, input: Self::InitAckIn) -> Result<Self::InitAckOut, Self::Error> {
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
        let agreed_resolution = {
            let mut res = Resolution::default();

            // Frame SN
            let i_fsn_res = init_ack.resolution.get(Field::FrameSN);
            let m_fsn_res = input.mine_resolution.get(Field::FrameSN);

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
            let m_rid_res = input.mine_resolution.get(Field::RequestID);

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
        let agreed_batch_size = input.mine_batch_size.min(init_ack.batch_size);

        // Extension QoS
        let qos_state = self
            .ext_qos
            .recv_init_ack((input.ext_qos, init_ack.qos))
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvInitAckOut {
            other_zid: init_ack.zid,
            other_whatami: init_ack.whatami,
            other_cookie: init_ack.cookie,
            agreed_resolution,
            agreed_batch_size,
            ext_qos: qos_state,
        };
        Ok(output)
    }

    type OpenSynIn = SendOpenSynIn;
    type OpenSynOut = SendOpenSynOut;
    async fn send_open_syn(&self, input: Self::OpenSynIn) -> Result<Self::OpenSynOut, Self::Error> {
        // Extension QoS
        let ext_qos = self
            .ext_qos
            .send_open_syn(input.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        // Build and send an OpenSyn message
        let mine_initial_sn = compute_sn(input.mine_zid, input.other_zid, input.agreed_resolution);
        let message: TransportMessage = OpenSyn {
            lease: input.mine_lease,
            initial_sn: mine_initial_sn,
            cookie: input.other_cookie,
            shm: None,  // @TODO
            auth: None, // @TODO
        }
        .into();

        let _ = self
            .link
            .send(&message)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = SendOpenSynOut {
            mine_initial_sn,
            ext_qos,
        };
        Ok(output)
    }

    type OpenAckIn = RecvOpenAckIn;
    type OpenAckOut = RecvOpenAckOut;
    async fn recv_open_ack(&self, input: Self::OpenAckIn) -> Result<Self::OpenAckOut, Self::Error> {
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
        let ext_qos = self
            .ext_qos
            .recv_open_ack(input.ext_qos)
            .await
            .map_err(|e| (e, Some(close::reason::GENERIC)))?;

        let output = RecvOpenAckOut {
            other_initial_sn: open_ack.initial_sn,
            other_lease: open_ack.lease,
            ext_qos,
        };
        Ok(output)
    }
}

pub(crate) async fn open_link(
    link: &LinkUnicast,
    manager: &TransportManager,
) -> ZResult<TransportUnicast> {
    // Extension QoS
    let ext_qos = ext::qos::QoS;

    let fsm = OpenLink { link, ext_qos };

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
        mine_resolution: manager.config.resolution,
        mine_batch_size: manager.config.batch_size,
        ext_qos: ext::qos::State {
            is_qos: manager.config.unicast.is_qos,
        },
    };
    let isyn_out = step!(fsm.send_init_syn(isyn_in).await);

    let iack_in = RecvInitAckIn {
        mine_resolution: isyn_out.mine_resolution,
        mine_batch_size: isyn_out.mine_batch_size,
        ext_qos: isyn_out.ext_qos,
    };
    let iack_out = step!(fsm.recv_init_ack(iack_in).await);

    // Open handshake
    let osyn_in = SendOpenSynIn {
        mine_zid: manager.config.zid,
        other_zid: iack_out.other_zid,
        mine_lease: manager.config.unicast.lease,
        agreed_resolution: iack_out.agreed_resolution,
        other_cookie: iack_out.other_cookie,
        ext_qos: iack_out.ext_qos,
    };
    let osyn_out = step!(fsm.send_open_syn(osyn_in).await);

    let oack_in = RecvOpenAckIn {
        ext_qos: osyn_out.ext_qos,
    };
    let oack_out = step!(fsm.recv_open_ack(oack_in).await);

    // Initialize the transport
    let config = TransportConfigUnicast {
        peer: iack_out.other_zid,
        whatami: iack_out.other_whatami,
        sn_resolution: iack_out.agreed_resolution.get(Field::FrameSN).mask(),
        tx_initial_sn: osyn_out.mine_initial_sn,
        is_qos: oack_out.ext_qos.is_qos,
        is_shm: false, // @TODO
    };
    let transport = step!(manager
        .init_transport_unicast(config)
        .await
        .map_err(|e| (e, Some(close::reason::INVALID))));

    // Finalize the transport
    // Add the link to the transport
    step!(step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .add_link(link.clone(), LinkUnicastDirection::Outbound)
    .map_err(|e| (e, Some(close::reason::MAX_LINKS))));

    // Sync the RX sequence number
    let _ = step!(transport
        .get_inner()
        .map_err(|e| (e, Some(close::reason::INVALID))))
    .sync(oack_out.other_initial_sn)
    .await;

    let output = InputFinalize {
        transport,
        other_lease: oack_out.other_lease,
        agreed_batch_size: iack_out.agreed_batch_size,
    };
    let transport = output.transport.clone();
    let res = finalize_transport(link, manager, output).await;
    if let Err(e) = res {
        let _ = transport.close().await;
        return Err(e);
    }

    log::debug!(
        "New transport link established with {}: {}",
        iack_out.other_zid,
        link
    );

    Ok(transport)
}
