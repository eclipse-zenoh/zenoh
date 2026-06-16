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

use std::sync::{Arc, atomic::AtomicU16};

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
pub struct ShmTransportMetadata {
    id_count: u64,
    challenge: u64,
    version: u64,
    protocols: [ProtocolID; 256],
    shm_counters: [AtomicU16; 1524],
}

impl ShmTransportMetadata {
    pub fn validate_challenge(&self, expected_challenge: AuthChallenge, s: &str) -> bool {
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
            tracing::debug!(
                "{} Version mismatch: ours: {}, theirs: {}.",
                s,
                SHM_VERSION,
                self.version
            );
            return false;
        }

        true
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
        self.segment
            .data
            .validate_challenge(expected_challenge, s)
    }

    pub fn challenge(&self) -> AuthChallenge {
        self.segment.data.challenge
    }

    pub fn segment_id(&self) -> AuthSegmentID {
        self.segment.data.id()
    }
}


#[derive(Debug)]
pub struct ShmTXCounterLease {
    segment: Arc<TXAuthSegment>,
    counter_index: ShmCounterID,
}

impl ShmTXCounterLease {
    pub fn new(segment: Arc<TXAuthSegment>) -> ZResult<Self> {
        let index = zlock!(segment.available_shm_counters).pop();
        index.map(|counter_index| Self {
            segment,
            counter_index,
        }).ok_or_else(|| zerror!("No available SHM counters").into())
    }

    pub fn id(&self) -> ShmCounterID {
        self.counter_index
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
        &self.segment.data.protocols[..self.segment.data.id_count as usize]
    }
}

#[derive(Debug)]
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

    pub fn counter_decrease(&mut self) {
        self.segment.segment.data.shm_counters[self.counter_index as usize].fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
