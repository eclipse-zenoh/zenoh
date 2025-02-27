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
use std::ops::Deref;

use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::bail;
use zenoh_crypto::PseudoRng;
use zenoh_protocol::transport::{init, open};
use zenoh_result::{zerror, Error as ZError, ZResult};
use zenoh_shm::{
    api::common::types::ProtocolID, posix_shm::array::ArrayInSHM, version::SHM_VERSION,
};

use crate::unicast::establishment::{AcceptFsm, OpenFsm};

/*************************************/
/*             Segment               */
/*************************************/
pub(crate) type AuthSegmentID = u32;
pub(crate) type AuthChallenge = u64;

const LEN_INDEX: usize = 0;
const CHALLENGE_INDEX: usize = 1;
const VERSION_INDEX: usize = 2;
const ID_START_INDEX: usize = 3;

#[derive(Debug)]
pub struct AuthSegment {
    array: ArrayInSHM<AuthSegmentID, AuthChallenge, usize>,
}

impl AuthSegment {
    pub fn create(challenge: AuthChallenge, shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        let array = ArrayInSHM::<AuthSegmentID, AuthChallenge, usize>::create(
            (ID_START_INDEX + shm_protocols.len()).try_into()?,
        )?;
        unsafe {
            (*array.elem_mut(LEN_INDEX)) = shm_protocols.len() as AuthChallenge;
            // challenge field is inverted to prevent SHM probing between new versioned
            // SHM implementation and the old one
            (*array.elem_mut(CHALLENGE_INDEX)) = !challenge;
            (*array.elem_mut(VERSION_INDEX)) = SHM_VERSION;
            #[allow(clippy::needless_range_loop)]
            for elem_index in 0..shm_protocols.len() {
                (*array.elem_mut(ID_START_INDEX + elem_index)) = shm_protocols[elem_index] as u64;
            }
        };
        Ok(Self { array })
    }

    pub fn open(id: AuthSegmentID) -> ZResult<Self> {
        let array = ArrayInSHM::open(id)?;

        // validate minimal array length
        if array.elem_count().get() < ID_START_INDEX {
            bail!("SHM auth segment is too small, maybe the other side is using an incompatible SHM version?")
        }

        Ok(Self { array })
    }

    pub fn challenge(&self) -> AuthChallenge {
        // challenge field is inverted to prevent SHM probing between new versioned
        // SHM implementation and the old one
        unsafe { !(*self.array.elem(CHALLENGE_INDEX)) }
    }

    pub fn validate_challenge(&self, expected_challenge: AuthChallenge, s: &str) -> bool {
        let challnge_in_shm = self.challenge();
        if challnge_in_shm != expected_challenge {
            tracing::debug!(
                "{} Challenge mismatch: expected: {}, found in shm: {}.",
                s,
                expected_challenge,
                challnge_in_shm
            );
            return false;
        }

        let version_in_shm = unsafe { *self.array.elem(VERSION_INDEX) };
        if version_in_shm != SHM_VERSION {
            tracing::debug!(
                "{} Version mismatch: ours: {}, theirs: {}.",
                s,
                SHM_VERSION,
                version_in_shm
            );
            return false;
        }

        true
    }

    pub fn protocols(&self) -> Vec<ProtocolID> {
        let mut result = vec![];
        let len = unsafe { (*self.array.elem(LEN_INDEX)) as usize };
        for elem in ID_START_INDEX..ID_START_INDEX + len {
            result.push(unsafe { *self.array.elem(elem) as ProtocolID });
        }
        result
    }

    pub fn id(&self) -> AuthSegmentID {
        self.array.id()
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
pub(crate) struct AuthUnicast {
    segment: AuthSegment,
}

impl Deref for AuthUnicast {
    type Target = AuthSegment;

    fn deref(&self) -> &Self::Target {
        &self.segment
    }
}

impl AuthUnicast {
    pub fn new(shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        // Create a challenge for session establishment
        let mut prng = PseudoRng::from_entropy();
        let nonce = prng.gen();

        // allocate SHM segment with challenge
        let segment = AuthSegment::create(nonce, shm_protocols)?;

        Ok(Self { segment })
    }
}

/*************************************/
/*             InitSyn               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  Segment id   ~
/// +---------------+
/// ```
pub(crate) struct InitSyn {
    pub(crate) alice_segment: AuthSegmentID,
}

// Codec
impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        self.write(&mut *writer, &x.alice_segment)?;
        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let alice_segment = self.read(&mut *reader)?;
        Ok(InitSyn { alice_segment })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+
/// ~  Segment id   ~
/// +---------------+
/// ```
struct InitAck {
    alice_challenge: u64,
    bob_segment: AuthSegmentID,
}

impl<W> WCodec<&InitAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        self.write(&mut *writer, x.alice_challenge)?;
        self.write(&mut *writer, &x.bob_segment)?;
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
        let bob_segment = self.read(&mut *reader)?;
        Ok(InitAck {
            alice_challenge,
            bob_segment,
        })
    }
}

// Extension Fsm
pub(crate) struct ShmFsm<'a> {
    inner: &'a AuthUnicast,
}

impl<'a> ShmFsm<'a> {
    pub(crate) const fn new(inner: &'a AuthUnicast) -> Self {
        Self { inner }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    // false by default, will be switched to true at the end of open_ack
    negotiated_to_use_shm: bool,
}

impl StateOpen {
    pub(crate) const fn new() -> Self {
        Self {
            negotiated_to_use_shm: false,
        }
    }

    pub(crate) const fn negotiated_to_use_shm(&self) -> bool {
        self.negotiated_to_use_shm
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            negotiated_to_use_shm: rng.gen_bool(0.5),
        }
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a ShmFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::Shm>;
    async fn send_init_syn(
        self,
        _state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        const S: &str = "Shm extension - Send InitSyn.";

        let init_syn = InitSyn {
            alice_segment: self.inner.id(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type RecvInitAckIn = Option<init::ext::Shm>;
    type RecvInitAckOut = Option<AuthSegment>;
    async fn recv_init_ack(
        self,
        mut input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitAck.";

        let Some(ext) = input.take() else {
            return Ok(None);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_ack): Result<InitAck, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            return Ok(None);
        };

        // Verify that Bob has correctly read Alice challenge
        if !self.inner.validate_challenge(init_ack.alice_challenge, S) {
            return Ok(None);
        }

        // Read Bob's SHM Segment
        let bob_segment = match AuthSegment::open(init_ack.bob_segment) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                return Ok(None);
            }
        };

        Ok(Some(bob_segment))
    }

    type SendOpenSynIn = &'a Self::RecvInitAckOut;
    type SendOpenSynOut = Option<open::ext::Shm>;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenSyn.";

        Ok(input
            .as_ref()
            .map(|val| open::ext::Shm::new(val.challenge())))
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::Shm>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenAck.";

        let (state, mut ext) = input;

        let Some(ext) = ext.take() else {
            return Ok(());
        };

        if ext.value != 1 {
            tracing::trace!("{} Invalid value.", S);
            return Ok(());
        }

        state.negotiated_to_use_shm = true;
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/

pub(crate) type StateAccept = StateOpen;

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        let negotiated_to_use_shm = u8::from(x.negotiated_to_use_shm);
        self.write(&mut *writer, negotiated_to_use_shm)?;
        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let negotiated_to_use_shm: u8 = self.read(&mut *reader)?;
        let negotiated_to_use_shm: bool = negotiated_to_use_shm == 1;
        Ok(StateAccept {
            negotiated_to_use_shm,
        })
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a ShmFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = Option<init::ext::Shm>;
    type RecvInitSynOut = Option<AuthSegment>;
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitSyn.";

        let Some(ext) = input.as_ref() else {
            return Ok(None);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_syn): Result<InitSyn, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            bail!("");
        };

        // Read Alice's SHM Segment
        let alice_segment = match AuthSegment::open(init_syn.alice_segment) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                return Ok(None);
            }
        };

        Ok(Some(alice_segment))
    }

    type SendInitAckIn = &'a Self::RecvInitSynOut;
    type SendInitAckOut = Option<init::ext::Shm>;
    async fn send_init_ack(
        self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Send InitAck.";

        let Some(alice_segment) = input.as_ref() else {
            return Ok(None);
        };

        let init_syn = InitAck {
            alice_challenge: alice_segment.challenge(),
            bob_segment: self.inner.id(),
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

        let Some(ext) = ext.take() else {
            return Ok(());
        };

        // Verify that Alice has correctly read Bob challenge
        let bob_challnge = ext.value;
        if !self.inner.validate_challenge(bob_challnge, S) {
            return Ok(());
        }

        state.negotiated_to_use_shm = true;

        Ok(())
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<open::ext::Shm>;
    async fn send_open_ack(
        self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenAck.";

        Ok(match input.negotiated_to_use_shm {
            true => Some(open::ext::Shm::new(1)),
            false => None,
        })
    }
}
