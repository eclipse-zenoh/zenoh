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
pub(crate) mod accept;
pub(super) mod cookie;
pub(super) mod ext;
pub(crate) mod open;

use super::super::TransportManager;
use super::{TransportPeer, TransportUnicast};
use async_trait::async_trait;
use cookie::*;
use sha3::{
    digest::{ExtendableOutput, Update, XofReader},
    Shake128,
};
use std::time::Duration;
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::{
    core::{Field, Resolution, ZInt, ZenohId},
    transport::{Close, TransportMessage},
};
use zenoh_result::ZResult;

/*************************************/
/*             TRAITS                */
/*************************************/
#[async_trait]
pub(crate) trait OpenFsm<'a> {
    type Error;

    type InitSynIn;
    type InitSynOut;
    async fn send_init_syn(
        &'a self,
        input: Self::InitSynIn,
    ) -> Result<Self::InitSynOut, Self::Error>;

    type InitAckIn;
    type InitAckOut;
    async fn recv_init_ack(
        &'a self,
        input: Self::InitAckIn,
    ) -> Result<Self::InitAckOut, Self::Error>;

    type OpenSynIn;
    type OpenSynOut;
    async fn send_open_syn(
        &'a self,
        input: Self::OpenSynIn,
    ) -> Result<Self::OpenSynOut, Self::Error>;

    type OpenAckIn;
    type OpenAckOut;
    async fn recv_open_ack(
        &'a self,
        input: Self::OpenAckIn,
    ) -> Result<Self::OpenAckOut, Self::Error>;
}

#[async_trait]
pub(crate) trait AcceptFsm {
    type Error;

    type InitSynIn;
    type InitSynOut;
    async fn recv_init_syn(&self, input: Self::InitSynIn) -> Result<Self::InitSynOut, Self::Error>;

    type InitAckIn;
    type InitAckOut;
    async fn send_init_ack(&self, input: Self::InitAckIn) -> Result<Self::InitAckOut, Self::Error>;

    type OpenSynIn;
    type OpenSynOut;
    async fn recv_open_syn(&self, input: Self::OpenSynIn) -> Result<Self::OpenSynOut, Self::Error>;

    type OpenAckIn;
    type OpenAckOut;
    async fn send_open_ack(&self, input: Self::OpenAckIn) -> Result<Self::OpenAckOut, Self::Error>;
}

/*************************************/
/*           FUNCTIONS               */
/*************************************/
pub(super) fn compute_sn(zid1: ZenohId, zid2: ZenohId, resolution: Resolution) -> ZInt {
    // Create a random yet deterministic initial_sn.
    // In case of multilink it's important that the same initial_sn is used for every connection attempt.
    // Instead of storing the state everywhere, we make sure that the we always compute the same initial_sn.
    let mut hasher = Shake128::default();
    hasher.update(zid1.as_slice());
    hasher.update(zid2.as_slice());
    let mut array = (0 as ZInt).to_le_bytes();
    hasher.finalize_xof().read(&mut array);
    ZInt::from_le_bytes(array) & resolution.get(Field::FrameSN).mask()
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
    pub(super) agreed_batch_size: u16,
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
    transport.start_tx(
        link,
        &manager.tx_executor,
        keep_alive,
        input.agreed_batch_size,
    )?;

    // Assign a callback if the transport is new
    // Keep the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = transport.get_alive().await;
    if transport.get_callback().is_none() {
        let peer = TransportPeer {
            zid: transport.get_zid(),
            whatami: transport.get_whatami(),
            is_qos: transport.is_qos(),
            is_shm: transport.is_shm(),
            links: vec![Link::from(link)],
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
