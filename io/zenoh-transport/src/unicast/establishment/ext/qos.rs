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
use core::marker::PhantomData;
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
pub(crate) struct QoS<'a> {
    _a: PhantomData<&'a ()>,
}

impl<'a> QoS<'a> {
    pub(crate) const fn new() -> Self {
        Self { _a: PhantomData }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[async_trait]
impl<'a> OpenFsm for QoS<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a State;
    type SendInitSynOut = Option<init::ext::QoS>;
    async fn send_init_syn(
        &self,
        state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let output = state.is_qos.then_some(init::ext::QoS::new());
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut State, Option<init::ext::QoS>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        &self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(())
    }

    type SendOpenSynIn = &'a State;
    type SendOpenSynOut = Option<open::ext::QoS>;
    async fn send_open_syn(
        &self,
        _state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        Ok(None)
    }

    type RecvOpenAckIn = (&'a mut State, Option<open::ext::QoS>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        &self,
        _state: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[async_trait]
impl<'a> AcceptFsm for QoS<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut State, Option<init::ext::QoS>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        &self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_qos &= other_ext.is_some();
        Ok(())
    }

    type SendInitAckIn = &'a State;
    type SendInitAckOut = Option<init::ext::QoS>;
    async fn send_init_ack(
        &self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        let output = state.is_qos.then_some(init::ext::QoS::new());
        Ok(output)
    }

    type RecvOpenSynIn = (&'a mut State, Option<open::ext::QoS>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        &self,
        _state: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        Ok(())
    }

    type SendOpenAckIn = &'a State;
    type SendOpenAckOut = Option<open::ext::QoS>;
    async fn send_open_ack(
        &self,
        _state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(None)
    }
}
