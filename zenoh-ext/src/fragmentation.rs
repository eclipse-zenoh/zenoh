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
//! Local assembly and recovery state for one pending publication.
//!
//! [`PendingSample`] distinguishes abandoned, whole, and fragmented publications.
//! [`FragmentAssembly`] stores fragments, tracks genuine progress, and selects
//! missing ranges for one recovery attempt at a time. Complete assemblies retain
//! their fragments until delivery, so late duplicates can still replace fragments.
//! Assembly completion preserves any in-flight attempt identity; finishing that
//! attempt clears it even if delivery is still pending. The subscriber owns
//! per-source ordering, query dispatch, and the decision to abandon a publication
//! that blocks delivery.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use zenoh::{bytes::ZBytes, internal::zerror, sample::Sample, Result as ZResult};

/// Default limit on the number of fragments per publication.
///
/// This bounds the fragment count, not bytes: memory use also depends on
/// payload and attachment sizes and assembly overhead. Override via
/// [`AdvancedSubscriberBuilder::max_fragments`](crate::AdvancedSubscriberBuilder::max_fragments).
pub(crate) const MAX_FRAGMENTS_DEFAULT: u32 = 4096;

/// Compute the number of fragments a `payload_len`-byte payload requires when
/// split in `size`-byte fragments, checking it fits `u32` (the wire encoding
/// of fragment metadata cannot represent more).
///
/// Subscriber-side fragment caps (for example
/// [`AdvancedSubscriberBuilder::max_fragments`](crate::AdvancedSubscriberBuilder::max_fragments),
/// defaulting to [`MAX_FRAGMENTS_DEFAULT`]) are enforced on the subscriber:
/// fragments beyond a subscriber's cap are rejected, so publishing more
/// fragments than a subscriber accepts results in that sample being dropped
/// by that subscriber.
///
/// The caller guarantees `payload_len > 0 && size > 0`.
pub(crate) fn fragment_count(payload_len: usize, size: usize) -> ZResult<u32> {
    u32::try_from(payload_len.div_ceil(size)).map_err(|_| {
        zerror!("payload of {payload_len} bytes would require more than u32::MAX fragments").into()
    })
}

/// First and last fragment numbers to request, both included.
///
/// An open end uses `None`, preserving the recovery query's wire representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FragRange {
    /// First fragment.
    pub(crate) start: u32,
    /// Last fragment, or the end of the sample when absent.
    pub(crate) end: Option<u32>,
}

impl FragRange {
    pub(crate) const fn new(start: u32, end: Option<u32>) -> Self {
        Self { start, end }
    }
}

/// What prompted a check for missing fragments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryTrigger {
    /// Query holes before received fragments, leaving the tail time to arrive.
    FragmentArrival,
    /// Also query the tail once no new fragment has arrived for the delay.
    TimerTick,
}

/// Recovery attempt state retained alongside the assembly until delivery.
#[derive(Debug, Clone, Default)]
pub(crate) enum FragmentRecovery {
    /// No attempt is running and recovery may start immediately.
    #[default]
    Idle,
    /// Identity of the running attempt, used to reject stale completions.
    InFlight(Arc<()>),
    /// When the last attempt failed. Wait before retrying. Give up on the
    /// sample only if it prevents delivery of a newer complete sample.
    FailedAt(Instant),
}

impl FragmentRecovery {
    fn can_start(&self, delay: Duration) -> bool {
        match self {
            Self::Idle => true,
            Self::InFlight(_) => false,
            Self::FailedAt(at) => at.elapsed() >= delay,
        }
    }

    fn start(&mut self) -> Arc<()> {
        let token = Arc::new(());
        *self = Self::InFlight(token.clone());
        token
    }
}

/// A publication buffered until it can be delivered in source order.
// Keeps unfragmented samples inline to avoid an extra allocation.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum PendingSample {
    /// Recovery failed, so we gave up on this sample to deliver a newer one.
    /// Keep this entry to reject late fragments until a newer sample is delivered
    /// or the buffer limit forces its removal.
    Abandoned,
    /// A complete publication received without multi-fragment assembly.
    Whole(Sample),
    /// An assembly that may be incomplete or waiting for ordered delivery.
    Fragmented(FragmentAssembly),
}

/// Fragment storage and local recovery bookkeeping for one publication.
///
/// Completing an assembly does not turn it into [`PendingSample::Whole`]: its
/// fragments remain replaceable by duplicates until [`PendingSample::into_sample`].
#[derive(Debug, Clone)]
pub(crate) struct FragmentAssembly {
    // TODO: `from_first_fragment` eagerly allocates `vec![None; frag_count]`
    //       (~600 KB per slot at the 4096-fragment cap, scaled by `max_pending_samples`),
    //       allowing a payload-less attacker to inflate memory by declaring
    //       a large `frag_count` on each first fragment.
    //       A sparse `BTreeMap<u32, Sample>`, or deferring allocation until
    //       a non-first fragment arrives, should be considered.
    /// Fragments indexed by their advertised fragment number.
    pub(crate) frags: Vec<Option<Sample>>,
    /// When we last received a fragment we did not already have.
    /// Used to decide when to query for the remaining fragments at the end.
    /// Receiving a duplicate does not update this time.
    pub(crate) last_progress: Instant,
    pub(crate) recovery: FragmentRecovery,
}

/// Reasons a fragment cannot create or update a pending publication.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum FragInsertError {
    /// The publication was abandoned and rejects late fragments.
    Abandoned,
    /// The fragment number is outside `0..frag_count`.
    InvalidFragNum { frag_count: u32, frag_num: u32 },
    /// The advertised fragment count is zero.
    InvalidFragCount { frag_count: u32 },
    /// The advertised fragment count differs from the pending publication's count.
    CountMismatch { expected: u32, got: u32 },
    /// The advertised fragment count exceeds the subscriber's limit.
    ExceedsMax { frag_count: u32, max: u32 },
}

impl PendingSample {
    #[inline]
    pub(crate) fn single(sample: Sample) -> Self {
        Self::Whole(sample)
    }

    /// Create a pending publication from its first received fragment.
    ///
    /// # Errors
    /// * [`FragInsertError::InvalidFragCount`] if `frag_count == 0`.
    /// * [`FragInsertError::InvalidFragNum`] if `frag_num >= frag_count`.
    /// * [`FragInsertError::ExceedsMax`] if `frag_count > max_fragments`.
    pub(crate) fn from_first_fragment(
        sample: Sample,
        frag_num: u32,
        frag_count: u32,
        max_fragments: u32,
    ) -> Result<Self, FragInsertError> {
        validate_fragment_metadata(frag_num, frag_count, max_fragments)?;
        if frag_count == 1 {
            return Ok(Self::Whole(sample));
        }
        Ok(Self::Fragmented(FragmentAssembly::from_first_fragment(
            sample, frag_num, frag_count,
        )))
    }

    #[inline]
    pub(crate) fn is_complete(&self) -> bool {
        match self {
            Self::Abandoned => false,
            Self::Whole(_) => true,
            Self::Fragmented(assembly) => assembly.is_complete(),
        }
    }

    pub(crate) fn is_abandoned(&self) -> bool {
        matches!(self, Self::Abandoned)
    }

    pub(crate) fn is_incomplete(&self) -> bool {
        matches!(self, Self::Fragmented(assembly) if !assembly.is_complete())
    }

    pub(crate) fn recovery_matches(&self, token: &Arc<()>) -> bool {
        matches!(self, Self::Fragmented(assembly) if assembly.recovery_matches(token))
    }

    pub(crate) fn is_retry(&self) -> bool {
        matches!(self, Self::Fragmented(assembly) if assembly.is_retry())
    }

    /// We can give up on this sample only if recovery failed and no new fragment
    /// has arrived since. Receiving a new fragment gives it another chance.
    pub(crate) fn can_abandon(&self) -> bool {
        matches!(self, Self::Fragmented(assembly) if assembly.can_abandon())
    }

    /// Find missing fragments to query and mark a recovery attempt as running.
    /// Return `None` if an attempt is already running, it is too soon to retry,
    /// or no fragments need to be queried yet.
    ///
    /// A received fragment triggers queries for missing earlier fragments.
    /// Timer ticks also query missing fragments at the end of the sample, once
    /// no new fragment has arrived for `delay`.
    pub(crate) fn prepare_recovery(
        &mut self,
        delay: Duration,
        trigger: RecoveryTrigger,
    ) -> Option<(Vec<FragRange>, Arc<()>)> {
        let Self::Fragmented(assembly) = self else {
            return None;
        };
        assembly.prepare_recovery(delay, trigger)
    }

    /// Mark the attempt as failed only if fragments it requested are still missing.
    /// Other fragments may still be arriving through publications.
    pub(crate) fn finish_recovery(&mut self, token: &Arc<()>, ranges: &[FragRange]) -> bool {
        let Self::Fragmented(assembly) = self else {
            return false;
        };
        assembly.finish_recovery(token, ranges)
    }

    /// Insert a fragment into this slot.
    ///
    /// Duplicate fragments overwrite previously stored ones, but do not reset
    /// the missing-tail recovery timer.
    ///
    /// # Errors
    /// * [`FragInsertError::InvalidFragCount`] if `frag_count == 0`.
    /// * [`FragInsertError::InvalidFragNum`] if `frag_num >= frag_count`.
    /// * [`FragInsertError::CountMismatch`] if the incoming count disagrees with the slot.
    /// * [`FragInsertError::Abandoned`] if recovery already gave up on this slot.
    pub(crate) fn insert(
        &mut self,
        sample: Sample,
        frag_num: u32,
        frag_count: u32,
    ) -> Result<(), FragInsertError> {
        validate_fragment(frag_num, frag_count)?;
        match self {
            Self::Abandoned => Err(FragInsertError::Abandoned),
            Self::Whole(current) => {
                if frag_count == 1 {
                    *current = sample;
                    return Ok(());
                }
                // Whole samples have no multi-fragment assembly. A later
                // fragment claiming a different count is inconsistent.
                Err(FragInsertError::CountMismatch {
                    expected: 1,
                    got: frag_count,
                })
            }
            Self::Fragmented(assembly) => assembly.insert(sample, frag_num, frag_count),
        }
    }

    /// Consume this slot and return the reassembled [`Sample`] if complete.
    pub(crate) fn into_sample(self) -> Option<Sample> {
        match self {
            Self::Abandoned => None,
            Self::Whole(sample) => Some(sample),
            Self::Fragmented(assembly) => assembly.into_sample(),
        }
    }
}

impl FragmentAssembly {
    /// The caller has validated the metadata and requires multiple fragments.
    fn from_first_fragment(sample: Sample, frag_num: u32, frag_count: u32) -> Self {
        let mut frags = vec![None; frag_count as usize];
        frags[frag_num as usize] = Some(sample);
        Self {
            frags,
            last_progress: Instant::now(),
            recovery: FragmentRecovery::Idle,
        }
    }

    fn is_complete(&self) -> bool {
        !self.frags.is_empty() && self.frags.iter().all(Option::is_some)
    }

    fn recovery_matches(&self, token: &Arc<()>) -> bool {
        matches!(&self.recovery, FragmentRecovery::InFlight(current) if Arc::ptr_eq(current, token))
    }

    fn is_retry(&self) -> bool {
        matches!(self.recovery, FragmentRecovery::FailedAt(_))
    }

    fn can_abandon(&self) -> bool {
        !self.is_complete()
            && matches!(&self.recovery, FragmentRecovery::FailedAt(failed_at)
                if self.last_progress <= *failed_at)
    }

    fn prepare_recovery(
        &mut self,
        delay: Duration,
        trigger: RecoveryTrigger,
    ) -> Option<(Vec<FragRange>, Arc<()>)> {
        if !self.recovery.can_start(delay) {
            return None;
        }
        let include_tail =
            trigger == RecoveryTrigger::TimerTick && self.last_progress.elapsed() >= delay;
        let mut ranges = missing_ranges_impl(&self.frags);
        if !include_tail && ranges.last().is_some_and(|range| range.end.is_none()) {
            ranges.pop();
        }
        if ranges.is_empty() {
            return None;
        }
        Some((ranges, self.recovery.start()))
    }

    fn finish_recovery(&mut self, token: &Arc<()>, ranges: &[FragRange]) -> bool {
        if !self.recovery_matches(token) {
            return false;
        }
        let failed = ranges.iter().any(|range| {
            let start = range.start as usize;
            let end = range
                .end
                .map(|n| n as usize + 1)
                .unwrap_or(self.frags.len());
            self.frags[start..end].iter().any(Option::is_none)
        });
        self.recovery = if failed {
            FragmentRecovery::FailedAt(Instant::now())
        } else {
            FragmentRecovery::Idle
        };
        failed
    }

    /// Insert validated fragment metadata, preserving progress on duplicates.
    fn insert(
        &mut self,
        sample: Sample,
        frag_num: u32,
        frag_count: u32,
    ) -> Result<(), FragInsertError> {
        if self.frags.len() != frag_count as usize {
            return Err(FragInsertError::CountMismatch {
                expected: self.frags.len() as u32,
                got: frag_count,
            });
        }
        let slot = &mut self.frags[frag_num as usize];
        let made_progress = slot.is_none();
        *slot = Some(sample);
        if made_progress {
            self.last_progress = Instant::now();
        }
        Ok(())
    }

    fn into_sample(self) -> Option<Sample> {
        if !self.is_complete() {
            return None;
        }
        let mut iter = self.frags.into_iter();
        let first = iter.next()??;
        let mut payload = ZBytes::writer();
        payload.append(first.payload().clone());
        for frag in iter {
            let frag = frag?;
            payload.append(frag.payload().clone());
        }
        Some(first.with_payload(payload.finish()))
    }
}

/// Validate advertised metadata without allocating an assembly or source state.
pub(crate) fn validate_fragment_metadata(
    frag_num: u32,
    frag_count: u32,
    max_fragments: u32,
) -> Result<(), FragInsertError> {
    validate_fragment(frag_num, frag_count)?;
    if frag_count > max_fragments {
        return Err(FragInsertError::ExceedsMax {
            frag_count,
            max: max_fragments,
        });
    }
    Ok(())
}

fn validate_fragment(frag_num: u32, frag_count: u32) -> Result<(), FragInsertError> {
    if frag_count == 0 {
        return Err(FragInsertError::InvalidFragCount { frag_count });
    }
    if frag_num >= frag_count {
        return Err(FragInsertError::InvalidFragNum {
            frag_count,
            frag_num,
        });
    }
    Ok(())
}

fn missing_ranges_impl(frags: &[Option<Sample>]) -> Vec<FragRange> {
    let mut missing_ranges = Vec::new();
    let mut hole_start = None;
    for (i, frag) in frags.iter().enumerate() {
        if frag.is_none() {
            hole_start.get_or_insert(i as u32);
            continue;
        }
        if let Some(start) = hole_start.take() {
            missing_ranges.push(FragRange::new(start, Some((i - 1) as u32)));
        }
    }
    if let Some(start) = hole_start {
        missing_ranges.push(FragRange::new(start, None));
    }
    missing_ranges
}

#[cfg(test)]
mod tests;
