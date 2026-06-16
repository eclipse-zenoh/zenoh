//
// Copyright (c) 2026 ZettaScale Technology
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
use std::{ops::Deref, sync::Arc};

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
use zenoh_shm::api::common::types::ProtocolID;

use crate::unicast::establishment::{
    ext::shm::shm_segment::{
        AuthChallenge, AuthSegmentID, RXAuthSegment, ShmCounterID, ShmTXCounterLease, TXAuthSegment,
    },
    AcceptFsm, OpenFsm,
};

/*************************************/
/*          Authenticator            */
/*************************************/
pub(crate) struct AuthUnicast {
    segment: Arc<TXAuthSegment>,
}

impl Deref for AuthUnicast {
    type Target = TXAuthSegment;

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
        let segment = Arc::new(TXAuthSegment::create(nonce, shm_protocols)?);

        Ok(Self { segment })
    }

    pub fn validate_challenge(&self, expected_challenge: AuthChallenge, s: &str) -> bool {
        self.segment.validate_challenge(expected_challenge, s)
    }

    pub fn challenge(&self) -> AuthChallenge {
        self.segment.challenge()
    }

    pub fn segment_id(&self) -> AuthSegmentID {
        self.segment.segment_id()
    }

    pub fn lease_tx_counter(&self) -> ZResult<ShmTXCounterLease> {
        ShmTXCounterLease::new(self.segment.clone())
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

/*************************************/
/*             OpenSyn               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+
/// ~  Counter Id   ~
/// +---------------+
/// ```
struct OpenSyn {
    bob_challenge: u64,
    alice_counter: ShmCounterID,
}

impl<W> WCodec<&OpenSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        self.write(&mut *writer, x.bob_challenge)?;
        self.write(&mut *writer, &x.alice_counter)?;
        Ok(())
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        let bob_challenge: u64 = self.read(&mut *reader)?;
        let alice_counter = self.read(&mut *reader)?;
        Ok(OpenSyn {
            bob_challenge,
            alice_counter,
        })
    }
}

/*************************************/
/*             OpenAck               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  Counter Id   ~
/// +---------------+
/// ```
struct OpenAck {
    bob_counter: ShmCounterID,
}

impl<W> WCodec<&OpenAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenAck) -> Self::Output {
        self.write(&mut *writer, &x.bob_counter)?;
        Ok(())
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        let bob_counter = self.read(&mut *reader)?;
        Ok(OpenAck { bob_counter })
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
#[derive(Debug, PartialEq, Eq)]
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

        // Send Alice's SHM Segment ID to Bob
        let init_syn = InitSyn {
            alice_segment: self.inner.segment_id(),
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
    type RecvInitAckOut = Option<(RXAuthSegment, ShmTXCounterLease)>;
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
        let bob_segment = match RXAuthSegment::open(init_ack.bob_segment) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                return Ok(None);
            }
        };

        // as long as Alice challenge correctly seen, allocate TX counter for this session
        let tx_counter_lease = self.inner.lease_tx_counter()?;

        Ok(Some((bob_segment, tx_counter_lease)))
    }

    type SendOpenSynIn = &'a Self::RecvInitAckOut;
    type SendOpenSynOut = Option<open::ext::Shm>;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenSyn.";

        Ok(input.as_ref().map(|(rx_segment, tx_counter)| {
            let open_syn = OpenSyn {
                bob_challenge: rx_segment.challenge(),
                alice_counter: tx_counter.counter_id(),
            };

            // Encode the extension
            let codec = Zenoh080::new();
            let mut buff = vec![];
            let mut writer = buff.writer();
            codec
                .write(&mut writer, &open_syn)
                .map_err(|_| zerror!("{} Encoding error", S))?;

            open::ext::Shm::new(buff.into())
        }))
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::Shm>);
    type RecvOpenAckOut = Option<ShmCounterID>;
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenAck.";

        let (state, mut ext) = input;

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(open_ack): Result<OpenAck, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            return Ok(None);
        };

        state.negotiated_to_use_shm = true;
        Ok(Some(open_ack.shm_counter_id))
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
    type RecvInitSynOut = Option<RXAuthSegment>;
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
        let alice_segment = match RXAuthSegment::open(init_syn.alice_segment) {
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

        let init_ack = InitAck {
            alice_challenge: alice_segment.challenge(),
            bob_segment: self.inner.segment_id(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_ack)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::Shm>);
    type RecvOpenSynOut = Option<(ShmTXCounterLease, ShmCounterID)>;
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenSyn.";

        let (state, mut ext) = input;

        let Some(ext) = input.as_ref() else {
            return Ok(None);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(open_syn): Result<OpenSyn, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            bail!("");
        };

        // Verify that Alice has correctly read Bob challenge
        if !self.inner.validate_challenge(bob_challnge, S) {
            return Ok(None);
        }

        // as long as Bob challenge correctly seen, allocate TX counter for this session
        let tx_counter_lease = self.inner.lease_tx_counter()?;

        state.negotiated_to_use_shm = true;

        Ok(Some((tx_counter_lease, open_syn.alice_counter)))
    }

    type SendOpenAckIn = (&'a StateAccept, &'a ShmTXCounterLease);
    type SendOpenAckOut = Option<open::ext::Shm>;
    async fn send_open_ack(
        self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        // const S: &str = "Shm extension - Send OpenAck.";

        let (state, lease) = input;

        let open_ack = OpenAck {
            bob_counter: lease.id()
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_ack)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(open::ext::Shm::new(buff.into())))
    }
}
