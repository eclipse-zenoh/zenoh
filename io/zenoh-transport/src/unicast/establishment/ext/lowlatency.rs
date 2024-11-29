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
use core::marker::PhantomData;

use async_trait::async_trait;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::transport::{init, open};
use zenoh_result::Error as ZError;

use crate::unicast::establishment::{AcceptFsm, OpenFsm};

// Extension Fsm
pub(crate) struct LowLatencyFsm<'a> {
    _a: PhantomData<&'a ()>,
}

impl LowLatencyFsm<'_> {
    pub(crate) const fn new() -> Self {
        Self { _a: PhantomData }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    is_lowlatency: bool,
}

impl StateOpen {
    pub(crate) const fn new(is_lowlatency: bool) -> Self {
        Self { is_lowlatency }
    }

    pub(crate) const fn is_lowlatency(&self) -> bool {
        self.is_lowlatency
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a LowLatencyFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::LowLatency>;
    async fn send_init_syn(
        self,
        state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let output = state.is_lowlatency.then_some(init::ext::LowLatency::new());
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<init::ext::LowLatency>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_lowlatency &= other_ext.is_some();
        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<open::ext::LowLatency>;
    async fn send_open_syn(
        self,
        _state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        Ok(None)
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::LowLatency>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        _state: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateAccept {
    is_lowlatency: bool,
}

impl StateAccept {
    pub(crate) const fn new(is_lowlatency: bool) -> Self {
        Self { is_lowlatency }
    }

    pub(crate) const fn is_lowlatency(&self) -> bool {
        self.is_lowlatency
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self::new(rng.gen_bool(0.5))
    }
}

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        let is_lowlatency = u8::from(x.is_lowlatency);
        self.write(&mut *writer, is_lowlatency)?;
        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let is_lowlatency: u8 = self.read(&mut *reader)?;
        let is_lowlatency = is_lowlatency == 1;
        Ok(StateAccept { is_lowlatency })
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a LowLatencyFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<init::ext::LowLatency>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        let (state, other_ext) = input;
        state.is_lowlatency &= other_ext.is_some();
        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<init::ext::LowLatency>;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        let output = state.is_lowlatency.then_some(init::ext::LowLatency::new());
        Ok(output)
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::LowLatency>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        _state: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        Ok(())
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<open::ext::LowLatency>;
    async fn send_open_ack(
        self,
        _state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(None)
    }
}
