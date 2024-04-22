// Copyright (c) 2024 ZettaScale Technology
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

use crate::unicast::{
    establishment::{AcceptFsm, OpenFsm},
    shared_memory_unicast::{Challenge, SharedMemoryUnicast},
};
use async_trait::async_trait;
use std::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::zasyncwrite;
use zenoh_protocol::transport::{init, open};
use zenoh_result::{zerror, Error as ZError};
use zenoh_shm::SharedMemoryBufInfo;

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~ ShmMemBufInfo ~
/// +---------------+
pub(crate) struct InitSyn {
    pub(crate) alice_info: SharedMemoryBufInfo,
}

// Codec
impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        self.write(&mut *writer, &x.alice_info)?;
        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let alice_info: SharedMemoryBufInfo = self.read(&mut *reader)?;
        Ok(InitSyn { alice_info })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+
/// ~ ShmMemBufInfo ~
/// +---------------+
struct InitAck {
    alice_challenge: u64,
    bob_info: SharedMemoryBufInfo,
}

impl<W> WCodec<&InitAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        self.write(&mut *writer, x.alice_challenge)?;
        self.write(&mut *writer, &x.bob_info)?;
        Ok(())
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        let alice_challenge: u64 = self.read(&mut *reader)?;
        let bob_info: SharedMemoryBufInfo = self.read(&mut *reader)?;
        Ok(InitAck {
            alice_challenge,
            bob_info,
        })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+

/*************************************/
/*             OpenAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~      ack      ~
/// +---------------+

// Extension Fsm
pub(crate) struct ShmFsm<'a> {
    inner: &'a SharedMemoryUnicast,
}

impl<'a> ShmFsm<'a> {
    pub(crate) const fn new(inner: &'a SharedMemoryUnicast) -> Self {
        Self { inner }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    is_shm: bool,
}

impl StateOpen {
    pub(crate) const fn new(is_shm: bool) -> Self {
        Self { is_shm }
    }

    pub(crate) const fn is_shm(&self) -> bool {
        self.is_shm
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a ShmFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::Shm>;
    async fn send_init_syn(
        self,
        state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        const S: &str = "Shm extension - Send InitSyn.";

        if !state.is_shm() {
            return Ok(None);
        }

        let init_syn = InitSyn {
            alice_info: self.inner.challenge.info.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<init::ext::Shm>);
    type RecvInitAckOut = Challenge;
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitAck.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(0);
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(0);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_ack): Result<InitAck, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            state.is_shm = false;
            return Ok(0);
        };

        // Alice challenge as seen by Alice
        let bytes: [u8; std::mem::size_of::<Challenge>()] = self
            .inner
            .challenge
            .as_slice()
            .try_into()
            .map_err(|e| zerror!("{}", e))?;
        let challenge = u64::from_le_bytes(bytes);

        // Verify that Bob has correctly read Alice challenge
        if challenge != init_ack.alice_challenge {
            tracing::trace!(
                "{} Challenge mismatch: {} != {}.",
                S,
                init_ack.alice_challenge,
                challenge
            );
            state.is_shm = false;
            return Ok(0);
        }

        // Read Bob's SharedMemoryBuf
        let shm_buff = match zasyncwrite!(self.inner.reader).read_shmbuf(&init_ack.bob_info) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                state.is_shm = false;
                return Ok(0);
            }
        };

        // Bob challenge as seen by Alice
        let bytes: [u8; std::mem::size_of::<Challenge>()] = match shm_buff.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(_) => {
                tracing::trace!("{} Failed to read remote Shm.", S);
                state.is_shm = false;
                return Ok(0);
            }
        };
        let bob_challenge = u64::from_le_bytes(bytes);

        Ok(bob_challenge)
    }

    type SendOpenSynIn = (&'a StateOpen, Self::RecvInitAckOut);
    type SendOpenSynOut = Option<open::ext::Shm>;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenSyn.";

        let (state, bob_challenge) = input;
        if !state.is_shm() {
            return Ok(None);
        }

        Ok(Some(open::ext::Shm::new(bob_challenge)))
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::Shm>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenAck.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(());
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(());
        };

        if ext.value != 1 {
            tracing::trace!("{} Invalid value.", S);
            state.is_shm = false;
            return Ok(());
        }

        state.is_shm = true;
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateAccept {
    is_shm: bool,
}

impl StateAccept {
    pub(crate) const fn new(is_shm: bool) -> Self {
        Self { is_shm }
    }

    pub(crate) const fn is_shm(&self) -> bool {
        self.is_shm
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
        let is_shm = u8::from(x.is_shm);
        self.write(&mut *writer, is_shm)?;
        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let is_shm: u8 = self.read(&mut *reader)?;
        let is_shm = is_shm == 1;
        Ok(StateAccept { is_shm })
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a ShmFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<init::ext::Shm>);
    type RecvInitSynOut = Challenge;
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitSyn.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(0);
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(0);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_syn): Result<InitSyn, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            state.is_shm = false;
            return Ok(0);
        };

        // Read Alice's SharedMemoryBuf
        let shm_buff = match zasyncwrite!(self.inner.reader).read_shmbuf(&init_syn.alice_info) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                state.is_shm = false;
                return Ok(0);
            }
        };

        // Alice challenge as seen by Bob
        let bytes: [u8; std::mem::size_of::<Challenge>()] = match shm_buff.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(_) => {
                tracing::trace!("{} Failed to read remote Shm.", S);
                state.is_shm = false;
                return Ok(0);
            }
        };
        let alice_challenge = u64::from_le_bytes(bytes);

        Ok(alice_challenge)
    }

    type SendInitAckIn = (&'a StateAccept, Self::RecvInitSynOut);
    type SendInitAckOut = Option<init::ext::Shm>;
    async fn send_init_ack(
        self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Send InitAck.";

        let (state, alice_challenge) = input;
        if !state.is_shm() {
            return Ok(None);
        }

        let init_syn = InitAck {
            alice_challenge,
            bob_info: self.inner.challenge.info.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::Shm>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenSyn.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(());
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(());
        };

        // Bob challenge as seen by Bob
        let bytes: [u8; std::mem::size_of::<Challenge>()] = self
            .inner
            .challenge
            .as_slice()
            .try_into()
            .map_err(|e| zerror!("{}", e))?;
        let challenge = u64::from_le_bytes(bytes);

        // Verify that Alice has correctly read Bob challenge
        let bob_challnge = ext.value;
        if challenge != bob_challnge {
            tracing::trace!(
                "{} Challenge mismatch: {} != {}.",
                S,
                bob_challnge,
                challenge
            );
            state.is_shm = false;
            return Ok(());
        }

        Ok(())
    }

    type SendOpenAckIn = &'a mut StateAccept;
    type SendOpenAckOut = Option<open::ext::Shm>;
    async fn send_open_ack(
        self,
        state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenAck.";

        if !state.is_shm() {
            return Ok(None);
        }

        state.is_shm = true;
        Ok(Some(open::ext::Shm::new(1)))
    }
}
