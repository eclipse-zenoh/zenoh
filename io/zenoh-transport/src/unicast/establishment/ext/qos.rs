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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::transport::{init, open};
use zenoh_result::Error as ZError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct State {
    is_qos: bool,
}

impl State {
    pub(crate) const fn new(is_qos: bool) -> Self {
        Self { is_qos }
    }

    pub(crate) const fn is_qos(&self) -> bool {
        self.is_qos
    }
}

// Codec
impl<W> WCodec<&State, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &State) -> Self::Output {
        let is_qos = u8::from(x.is_qos);
        self.write(&mut *writer, is_qos)?;
        Ok(())
    }
}

impl<R> RCodec<State, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<State, Self::Error> {
        let is_qos: u8 = self.read(&mut *reader)?;
        let is_qos = is_qos == 1;
        Ok(State { is_qos })
    }
}

// Extension Fsm
pub(crate) struct QoS;

impl QoS {
    pub(crate) const fn new() -> Self {
        Self
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[async_trait]
impl<'a> OpenFsm<'a> for QoS {
    type Error = ZError;

    type InitSynIn = &'a State;
    type InitSynOut = Option<init::ext::QoS>;
    async fn send_init_syn(
        &'a self,
        state: Self::InitSynIn,
    ) -> Result<Self::InitSynOut, Self::Error> {
        let output = state.is_qos.then_some(init::ext::QoS::new());
        Ok(output)
    }

    type InitAckIn = (&'a mut State, Option<init::ext::QoS>);
    type InitAckOut = ();
    async fn recv_init_ack(
        &'a self,
        input: Self::InitAckIn,
    ) -> Result<Self::InitAckOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(())
    }

    type OpenSynIn = &'a State;
    type OpenSynOut = Option<open::ext::QoS>;
    async fn send_open_syn(
        &'a self,
        _state: Self::OpenSynIn,
    ) -> Result<Self::OpenSynOut, Self::Error> {
        Ok(None)
    }

    type OpenAckIn = (&'a mut State, Option<open::ext::QoS>);
    type OpenAckOut = ();
    async fn recv_open_ack(
        &'a self,
        _state: Self::OpenAckIn,
    ) -> Result<Self::OpenAckOut, Self::Error> {
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[async_trait]
impl<'a> AcceptFsm<'a> for QoS {
    type Error = ZError;

    type InitSynIn = (&'a mut State, Option<init::ext::QoS>);
    type InitSynOut = ();
    async fn recv_init_syn(
        &'a self,
        input: Self::InitSynIn,
    ) -> Result<Self::InitSynOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(())
    }

    type InitAckIn = &'a State;
    type InitAckOut = Option<init::ext::QoS>;
    async fn send_init_ack(
        &'a self,
        state: Self::InitAckIn,
    ) -> Result<Self::InitAckOut, Self::Error> {
        let output = state.is_qos.then_some(init::ext::QoS::new());
        Ok(output)
    }

    type OpenSynIn = (&'a mut State, Option<open::ext::QoS>);
    type OpenSynOut = ();
    async fn recv_open_syn(
        &'a self,
        _state: Self::OpenSynIn,
    ) -> Result<Self::OpenSynOut, Self::Error> {
        Ok(())
    }

    type OpenAckIn = &'a State;
    type OpenAckOut = Option<open::ext::QoS>;
    async fn send_open_ack(
        &'a self,
        _state: Self::OpenAckIn,
    ) -> Result<Self::OpenAckOut, Self::Error> {
        Ok(None)
    }
}
