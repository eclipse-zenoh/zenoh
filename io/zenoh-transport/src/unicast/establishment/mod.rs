//
// Copyright (c) 2023 ZettaScale Technology
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
pub(crate) mod accept;
pub(super) mod cookie;
pub mod ext;
pub(crate) mod open;

use crate::common::seq_num;
use async_trait::async_trait;
use cookie::*;
use sha3::{
    digest::{ExtendableOutput, Update, XofReader},
    Shake128,
};
use zenoh_protocol::{
    core::{Field, Resolution, ZenohId},
    transport::TransportSn,
};

/*************************************/
/*             TRAITS                */
/*************************************/
#[async_trait]
pub trait OpenFsm {
    type Error;

    type SendInitSynIn;
    type SendInitSynOut;
    async fn send_init_syn(
        self,
        input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error>;

    type RecvInitAckIn;
    type RecvInitAckOut;
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error>;

    type SendOpenSynIn;
    type SendOpenSynOut;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error>;

    type RecvOpenAckIn;
    type RecvOpenAckOut;
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error>;
}

#[async_trait]
pub trait AcceptFsm {
    type Error;

    type RecvInitSynIn;
    type RecvInitSynOut;
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error>;

    type SendInitAckIn;
    type SendInitAckOut;
    async fn send_init_ack(
        self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error>;

    type RecvOpenSynIn;
    type RecvOpenSynOut;
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error>;

    type SendOpenAckIn;
    type SendOpenAckOut;
    async fn send_open_ack(
        self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error>;
}

/*************************************/
/*           FUNCTIONS               */
/*************************************/
pub(super) fn compute_sn(zid1: ZenohId, zid2: ZenohId, resolution: Resolution) -> TransportSn {
    // Create a random yet deterministic initial_sn.
    // In case of multilink it's important that the same initial_sn is used for every connection attempt.
    // Instead of storing the state everywhere, we make sure that the we always compute the same initial_sn.
    let mut hasher = Shake128::default();
    hasher.update(&zid1.to_le_bytes()[..zid1.size()]);
    hasher.update(&zid2.to_le_bytes()[..zid2.size()]);
    let mut array = (0 as TransportSn).to_le_bytes();
    hasher.finalize_xof().read(&mut array);
    TransportSn::from_le_bytes(array) & seq_num::get_mask(resolution.get(Field::FrameSN))
}

pub(super) async fn close_link(link: &LinkUnicast, reason: Option<u8>) {
    if let Some(reason) = reason {
        // Build the close message
        let message: TransportMessage = Close {
            reason,
            session: false,
        }
        .into();
        // Send the close message on the link
        let _ = link.send(&message).await;
    }

    // Close the link
    let _ = link.close().await;
}

pub(super) struct InputFinalize {
    pub(super) transport: TransportUnicast,
    pub(super) other_lease: Duration,
    pub(super) agreed_batch_size: BatchSize,
}
// Finalize the transport, notify the callback and start the link tasks
pub(super) async fn finalize_transport(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: self::InputFinalize,
) -> ZResult<()> {
    // Retrive the transport's transport
    let transport = input.transport.get_inner()?;

    // Start the TX loop
    let keep_alive = manager.config.unicast.lease / manager.config.unicast.keep_alive as u32;
    transport.start_tx(link, keep_alive, input.agreed_batch_size)?;

    // Assign a callback if the transport is new
    // Keep the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = transport.get_alive().await;
    if transport.get_callback().is_none() {
        let peer = TransportPeer {
            zid: transport.get_zid(),
            whatami: transport.get_whatami(),
            links: vec![Link::from(link)],
            is_qos: transport.is_qos(),
            #[cfg(feature = "shared-memory")]
            is_shm: transport.is_shm(),
        };
        // Notify the transport handler that there is a new transport and get back a callback
        // NOTE: the read loop of the link the open message was sent on remains blocked
        //       until new_unicast() returns. The read_loop in the various links
        //       waits for any eventual transport to associate to.
        let callback = manager
            .config
            .handler
            .new_unicast(peer, input.transport.clone())?;
        // Set the callback on the transport
        transport.set_callback(callback);
    }
    if let Some(callback) = transport.get_callback() {
        // Notify the transport handler there is a new link on this transport
        callback.new_link(Link::from(link));
    }
    drop(a_guard);

    // Start the RX loop
    transport.start_rx(link, input.other_lease, input.agreed_batch_size)?;

    Ok(())
}
