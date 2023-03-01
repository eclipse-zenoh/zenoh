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
use crate::unicast::establishment::{AcceptFsm, OpenFsm};
use async_trait::async_trait;
use zenoh_protocol::transport::init;
use zenoh_result::Error as ZError;

pub(crate) struct QoS;

pub(crate) struct State {
    pub(crate) is_qos: bool,
}

/*************************************/
/*              OPEN                 */
/*************************************/

#[async_trait]
impl OpenFsm for QoS {
    type Error = ZError;

    type InitSynIn = State;
    type InitSynOut = (State, Option<init::ext::QoS>);
    async fn send_init_syn(&self, state: Self::InitSynIn) -> Result<Self::InitSynOut, Self::Error> {
        let mine_ext = state.is_qos.then_some(init::ext::QoS::new());
        let output = (state, mine_ext);
        Ok(output)
    }

    type InitAckIn = (State, Option<init::ext::QoS>);
    type InitAckOut = State;
    async fn recv_init_ack(&self, input: Self::InitAckIn) -> Result<Self::InitAckOut, Self::Error> {
        let (mut state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(state)
    }

    type OpenSynIn = State;
    type OpenSynOut = State;
    async fn send_open_syn(&self, state: Self::OpenSynIn) -> Result<Self::OpenSynOut, Self::Error> {
        Ok(state)
    }

    type OpenAckIn = State;
    type OpenAckOut = State;
    async fn recv_open_ack(&self, state: Self::OpenAckIn) -> Result<Self::OpenAckOut, Self::Error> {
        Ok(state)
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[async_trait]
impl AcceptFsm for QoS {
    type Error = ZError;

    type InitSynIn = (State, Option<init::ext::QoS>);
    type InitSynOut = State;
    async fn recv_init_syn(&self, input: Self::InitSynIn) -> Result<Self::InitSynOut, Self::Error> {
        let (mut state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(state)
    }

    type InitAckIn = State;
    type InitAckOut = (State, Option<init::ext::QoS>);
    async fn send_init_ack(&self, state: Self::InitAckIn) -> Result<Self::InitAckOut, Self::Error> {
        let mine_ext = state.is_qos.then_some(init::ext::QoS::new());
        let output = (state, mine_ext);
        Ok(output)
    }

    type OpenSynIn = State;
    type OpenSynOut = State;
    async fn recv_open_syn(&self, state: Self::OpenSynIn) -> Result<Self::OpenSynOut, Self::Error> {
        Ok(state)
    }

    type OpenAckIn = State;
    type OpenAckOut = State;
    async fn send_open_ack(&self, state: Self::OpenAckIn) -> Result<Self::OpenAckOut, Self::Error> {
        Ok(state)
    }
}
