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
use std::{cmp::min, marker::PhantomData};

use async_trait::async_trait;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::transport::init::ext::PatchType;
use zenoh_result::{bail, Error as ZError};

use crate::unicast::establishment::{AcceptFsm, OpenFsm};

// Extension Fsm
pub(crate) struct PatchFsm<'a> {
    _a: PhantomData<&'a ()>,
}

impl PatchFsm<'_> {
    pub(crate) const fn new() -> Self {
        Self { _a: PhantomData }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    patch: PatchType,
}

impl StateOpen {
    pub(crate) const fn new() -> Self {
        Self {
            patch: PatchType::NONE,
        }
    }

    pub(crate) const fn get(&self) -> PatchType {
        self.patch
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a PatchFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = PatchType;
    async fn send_init_syn(
        self,
        _state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        Ok(PatchType::CURRENT)
    }

    type RecvInitAckIn = (&'a mut StateOpen, PatchType);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (state, other_ext) = input;
        if other_ext > PatchType::CURRENT {
            bail!(
                "Acceptor patch should be lesser or equal to {current:?}, found {other:?}",
                current = PatchType::CURRENT.raw(),
                other = other_ext.raw(),
            );
        }
        state.patch = other_ext;
        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = ();
    async fn send_open_syn(
        self,
        _state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        unimplemented!("There is no patch extension in OPEN")
    }

    type RecvOpenAckIn = (&'a mut StateOpen, ());
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        _state: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        unimplemented!("There is no patch extension in OPEN")
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateAccept {
    patch: PatchType,
}

impl StateAccept {
    pub(crate) const fn new() -> Self {
        Self {
            patch: PatchType::NONE,
        }
    }

    pub(crate) const fn get(&self) -> PatchType {
        self.patch
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        Self {
            patch: PatchType::rand(),
        }
    }
}

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        let raw = x.patch.raw();
        self.write(&mut *writer, raw)?;
        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let raw: u8 = self.read(&mut *reader)?;
        let patch = PatchType::new(raw);
        Ok(StateAccept { patch })
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a PatchFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, PatchType);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        let (state, other_ext) = input;
        state.patch = other_ext;
        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = PatchType;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(min(PatchType::CURRENT, state.patch))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, ());
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        _state: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        unimplemented!("There is no patch extension in OPEN")
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = ();
    async fn send_open_ack(
        self,
        _state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        unimplemented!("There is no patch extension in OPEN")
    }
}
