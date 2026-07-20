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
use zenoh_protocol::{
    core::Reliability,
    transport::{init, open},
};
use zenoh_result::{zerror, Error as ZError, ZResult};
use zenoh_shm::api::common::types::ProtocolID;

use crate::{
    common::shm::interop::{LinkShmHandoffConfig, TransportShmConfig},
    unicast::establishment::{
        ext::shm::{
            handoff::{HandoffCounterIds, RxHandoffChannel, TxHandoffChannel},
            segment::{
                AuthChallenge, AuthSegmentID, RXAuthSegment, ShmTXCounterLease, TXAuthSegment,
            },
        },
        AcceptFsm, OpenFsm,
    },
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
    alice_counters: HandoffCounterIds,
}

impl<W> WCodec<&OpenSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        self.write(&mut *writer, x.bob_challenge)?;
        self.write(&mut *writer, &x.alice_counters)?;
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
        let alice_counters = self.read(&mut *reader)?;
        Ok(OpenSyn {
            bob_challenge,
            alice_counters,
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
    bob_counters: HandoffCounterIds,
}

impl<W> WCodec<&OpenAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenAck) -> Self::Output {
        self.write(&mut *writer, &x.bob_counters)?;
        Ok(())
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        let bob_counters = self.read(&mut *reader)?;
        Ok(OpenAck { bob_counters })
    }
}

// Extension Fsm
pub(crate) struct ShmFsm<'a> {
    inner: &'a AuthUnicast,
    tx_lease: std::sync::OnceLock<TxHandoffChannel>,
    rx_segment: std::sync::OnceLock<Arc<RXAuthSegment>>,
    rx_lease: std::sync::OnceLock<RxHandoffChannel>,
}

impl<'a> ShmFsm<'a> {
    pub(crate) const fn new(inner: &'a AuthUnicast) -> Self {
        Self {
            inner,
            tx_lease: std::sync::OnceLock::new(),
            rx_segment: std::sync::OnceLock::new(),
            rx_lease: std::sync::OnceLock::new(),
        }
    }

    pub fn shm_init_result(self) -> (Option<TransportShmConfig>, LinkShmHandoffConfig) {
        let transport = self.rx_segment.into_inner().map(TransportShmConfig::new);
        let link = self
            .rx_lease
            .into_inner()
            .zip(self.tx_lease.into_inner())
            .map(|(rx_lease, tx_lease)| LinkShmHandoffConfig::new(rx_lease, tx_lease))
            .unwrap_or(LinkShmHandoffConfig::new_disaled());

        (transport, link)
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {}

impl StateOpen {
    pub(crate) const fn new() -> Self {
        Self {}
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        Self {}
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
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        mut input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitAck.";

        let Some(ext) = input.take() else {
            return Ok(());
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_ack): Result<InitAck, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            return Ok(());
        };

        // Verify that Bob has correctly read Alice challenge
        if !self.inner.validate_challenge(init_ack.alice_challenge, S) {
            return Ok(());
        }

        // Read Bob's SHM Segment
        let bob_segment = match RXAuthSegment::open(init_ack.bob_segment) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                return Ok(());
            }
        };

        // Store Bob's SHM Segment for later use
        self.rx_segment
            .set(Arc::new(bob_segment))
            .map_err(|_| zerror!("{} State machine error", S))?;

        Ok(())
    }

    type SendOpenSynIn = Reliability;
    type SendOpenSynOut = Option<open::ext::Shm>;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Send OpenSyn.";

        // Link is multipriority
        let reliability = input;

        // take RX segment from input
        let Some(rx_segment) = self.rx_segment.get() else {
            return Ok(None);
        };

        // Allocate TX counter for this session
        let tx_handoff = TxHandoffChannel::new_tx(reliability, &self.inner)?;

        let open_syn = OpenSyn {
            bob_challenge: rx_segment.challenge(),
            alice_counters: tx_handoff.ids(),
        };

        // Encode the extension
        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        self.tx_lease
            .set(tx_handoff)
            .map_err(|_| zerror!("{} State machine error", S))?;

        Ok(Some(open::ext::Shm::new(buff.into())))
    }

    type RecvOpenAckIn = Option<open::ext::Shm>;
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        mut input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenAck.";

        let Some(ext) = input.take() else {
            return Ok(());
        };

        let Some(segment) = self.rx_segment.get() else {
            return Ok(());
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(open_ack): Result<OpenAck, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            return Ok(());
        };

        let rx_handoff = RxHandoffChannel::new_rx(segment, open_ack.bob_counters);

        self.rx_lease
            .set(rx_handoff)
            .map_err(|_| zerror!("{} State machine error", S))?;

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

    fn write(self, _writer: &mut W, _x: &StateAccept) -> Self::Output {
        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, _reader: &mut R) -> Result<StateAccept, Self::Error> {
        Ok(StateAccept {})
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a ShmFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = Option<init::ext::Shm>;
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitSyn.";

        let Some(ext) = input.as_ref() else {
            return Ok(());
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_syn): Result<InitSyn, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            return Ok(());
        };

        // Read Alice's SHM Segment
        let alice_segment = match RXAuthSegment::open(init_syn.alice_segment) {
            Ok(buff) => buff,
            Err(e) => {
                tracing::trace!("{} {}", S, e);
                return Ok(());
            }
        };

        self.rx_segment
            .set(Arc::new(alice_segment))
            .map_err(|_| zerror!("{} State machine error", S))?;

        Ok(())
    }

    type SendInitAckIn = ();
    type SendInitAckOut = Option<init::ext::Shm>;
    async fn send_init_ack(
        self,
        _input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        const S: &str = "Shm extension - Send InitAck.";

        let Some(alice_segment) = self.rx_segment.get() else {
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

    type RecvOpenSynIn = Option<open::ext::Shm>;
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenSyn.";

        let Some(ext) = input else {
            return Ok(());
        };

        let Some(segment) = self.rx_segment.get() else {
            return Ok(());
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(open_syn): Result<OpenSyn, _> = codec.read(&mut reader) else {
            tracing::trace!("{} Decoding error.", S);
            bail!("");
        };

        // Verify that Alice has correctly read Bob challenge
        if !self.inner.validate_challenge(open_syn.bob_challenge, S) {
            return Ok(());
        }

        // Allocate RX counter for this session
        let rx_handoff = RxHandoffChannel::new_rx(segment, open_syn.alice_counters);

        self.rx_lease
            .set(rx_handoff)
            .map_err(|_| zerror!("{} State machine error", S))?;

        Ok(())
    }

    type SendOpenAckIn = Reliability;
    type SendOpenAckOut = Option<open::ext::Shm>;
    async fn send_open_ack(
        self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Send OpenAck.";

        // Link is multipriority
        let reliability = input;

        // Allocate TX counter for this session
        let tx_handoff = TxHandoffChannel::new_tx(reliability, &self.inner)?;

        let open_ack = OpenAck {
            bob_counters: tx_handoff.ids(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_ack)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        self.tx_lease
            .set(tx_handoff)
            .map_err(|_| zerror!("{} State machine error", S))?;

        Ok(Some(open::ext::Shm::new(buff.into())))
    }
}
