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

use std::{
    ops::Deref, sync::{Arc, atomic::AtomicU32},
};

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{bail, zerror, zlock};
use zenoh_result::ZResult;
use zenoh_shm::{
    api::common::types::ProtocolID, posix_shm::struct_in_shm::StructInSHM, version::SHM_VERSION,
};

/*************************************/
/*             Segment               */
/*************************************/
pub(crate) type AuthSegmentID = u32;
pub(crate) type AuthChallenge = u64;
pub(crate) type ShmCounterID = u16;

#[derive(Debug)]
#[repr(C)]
struct ShmTransportMetadata {
    id_count: u64,
    challenge: u64,
    version: u64,
    protocols: [ProtocolID; 256],
    shm_counters: [AtomicU32; 762],
}

impl ShmTransportMetadata {
    fn validate_challenge(&self, expected_challenge: AuthChallenge, s: &str) -> bool {
        if self.challenge != expected_challenge {
            tracing::debug!(
                "{} Challenge mismatch: expected: {}, found in shm: {}.",
                s,
                expected_challenge,
                self.challenge
            );
            return false;
        }

        if self.version != SHM_VERSION {
            tracing::warn!(
                "{} Version mismatch: ours: {}, theirs: {}.",
                s,
                SHM_VERSION,
                self.version
            );
            return false;
        }

        true
    }

    fn protocols(&self) -> &[ProtocolID] {
        &self.protocols[..self.id_count as usize]
    }

    fn counter(&self, id: ShmCounterID) -> &AtomicU32 {
        &self.shm_counters[id as usize]
    }
}

#[derive(Debug)]
struct ShmTransportMetadataSegment {
    data: StructInSHM<AuthSegmentID, ShmTransportMetadata>,
}

impl PartialEq for ShmTransportMetadataSegment {
    fn eq(&self, other: &Self) -> bool {
        self.data.id() == other.data.id()
    }
}

impl Eq for ShmTransportMetadataSegment {}

impl ShmTransportMetadataSegment {
    fn create(challenge: AuthChallenge, shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        let mut data: StructInSHM<AuthSegmentID, ShmTransportMetadata> = StructInSHM::create()?;

        if data.protocols.len() < shm_protocols.len() {
            bail!(
                "Too many protocols: {}. Max is {}",
                shm_protocols.len(),
                data.protocols.len()
            );
        }

        data.id_count = shm_protocols.len() as u64;
        data.challenge = challenge;
        data.version = SHM_VERSION;
        data.protocols[..shm_protocols.len()].copy_from_slice(shm_protocols);
        data.shm_counters
            .iter()
            .for_each(|counter| counter.store(0, std::sync::atomic::Ordering::Relaxed));

        Ok(Self { data })
    }

    fn open(id: AuthSegmentID) -> ZResult<Self> {
        let data = StructInSHM::open(id)?;
        Ok(Self { data })
    }
}

#[derive(Debug)]
pub struct TXAuthSegment {
    segment: ShmTransportMetadataSegment,
    available_shm_counters: Arc<std::sync::Mutex<Vec<ShmCounterID>>>,
}

impl PartialEq for TXAuthSegment {
    fn eq(&self, other: &Self) -> bool {
        self.segment == other.segment
    }
}

impl Eq for TXAuthSegment {}

impl TXAuthSegment {
    pub fn create(challenge: AuthChallenge, shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        let segment = ShmTransportMetadataSegment::create(challenge, shm_protocols)?;
        let available_shm_counters = Arc::new(std::sync::Mutex::new(
            (0..segment.data.shm_counters.len())
                .map(|index| index as ShmCounterID)
                .collect(),
        ));

        Ok(Self {
            segment,
            available_shm_counters,
        })
    }

    pub fn validate_challenge(&self, expected_challenge: AuthChallenge, s: &str) -> bool {
        self.segment.data.validate_challenge(expected_challenge, s)
    }

    pub fn segment_id(&self) -> AuthSegmentID {
        self.segment.data.id()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShmTXCounterLease {
    segment: Arc<TXAuthSegment>,
    counter_index: ShmCounterID,
}

impl ShmTXCounterLease {
    pub fn new(segment: Arc<TXAuthSegment>) -> ZResult<Self> {
        let index = zlock!(segment.available_shm_counters).pop();
        index
            .map(|counter_index| Self {
                segment,
                counter_index,
            })
            .ok_or_else(|| zerror!("No available SHM counters").into())
    }

    pub fn id(&self) -> ShmCounterID {
        self.counter_index
    }

    pub fn counter_increase(&self) {
        self.segment
            .segment
            .data
            .counter(self.counter_index)
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn counter(&self) -> u32 {
        self.segment
            .segment
            .data
            .counter(self.counter_index)
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Drop for ShmTXCounterLease {
    fn drop(&mut self) {
        zlock!(self.segment.available_shm_counters).push(self.counter_index);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RXAuthSegment {
    segment: ShmTransportMetadataSegment,
}

impl RXAuthSegment {
    pub fn open(id: AuthSegmentID) -> ZResult<Self> {
        let segment = ShmTransportMetadataSegment::open(id)?;
        Ok(Self { segment })
    }

    pub fn challenge(&self) -> AuthChallenge {
        self.segment.data.challenge
    }

    pub fn protocols(&self) -> &[ProtocolID] {
        self.segment.data.protocols()
    }
}

// Codec
impl<W> WCodec<&RXAuthSegment, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &RXAuthSegment) -> Self::Output {
        self.write(writer, &x.segment.data)
    }
}

impl<R> RCodec<RXAuthSegment, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<RXAuthSegment, Self::Error> {
        let segment_data = self.read(&mut *reader)?;
        Ok(RXAuthSegment {
            segment: ShmTransportMetadataSegment { data: segment_data },
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShmRXCounterLease {
    segment: Arc<RXAuthSegment>,
    counter_index: ShmCounterID,
}

impl ShmRXCounterLease {
    pub fn new(segment: Arc<RXAuthSegment>, counter_index: ShmCounterID) -> Self {
        Self {
            segment,
            counter_index,
        }
    }

    pub fn counter_decrease(&self) {
        self.segment
            .segment
            .data
            .counter(self.counter_index)
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn segment(&self) -> &RXAuthSegment {
        &self.segment
    }
}

impl<W> WCodec<&ShmRXCounterLease, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ShmRXCounterLease) -> Self::Output {
        self.write(&mut *writer, x.segment.deref())?;
        self.write(writer, &x.counter_index)
    }
}

impl<R> RCodec<ShmRXCounterLease, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ShmRXCounterLease, Self::Error> {
        let segment_data = self.read(&mut *reader)?;
        let counter_index = self.read(reader)?;
        Ok(ShmRXCounterLease {
            segment: Arc::new(RXAuthSegment {
                segment: ShmTransportMetadataSegment { data: segment_data },
            }),
            counter_index,
        })
    }
}
