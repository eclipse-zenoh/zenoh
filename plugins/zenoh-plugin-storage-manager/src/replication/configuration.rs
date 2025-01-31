//
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
//

use std::{
    ops::Deref,
    time::{SystemTime, UNIX_EPOCH},
};

use zenoh::{internal::bail, key_expr::OwnedKeyExpr, time::Timestamp, Result};
use zenoh_backend_traits::config::ReplicaConfig;

use super::{
    classification::{IntervalIdx, SubIntervalIdx},
    digest::Fingerprint,
};

/// The [Configuration] is, mostly, a thin wrapper around the [ReplicaConfig].
///
/// It exposes its fingerprint: a 64 bits hash of its inner fields. The `storage_key_expr` (and
/// specifically the fact that it is used to compute the fingerprint) is here to prevent having
/// replicas that are active on different subset to exchange their Digest. We want to avoid having
/// a Replica active on "replication/**" to receive and process the Digests emitted by a Replica
/// active on "replication/a/*".
///
/// Using the newtype pattern allows us to add methods to compute the time classification of
/// events.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Configuration {
    storage_key_expr: OwnedKeyExpr,
    prefix: Option<OwnedKeyExpr>,
    replica_config: ReplicaConfig,
    fingerprint: Fingerprint,
}

impl Deref for Configuration {
    type Target = ReplicaConfig;

    fn deref(&self) -> &Self::Target {
        &self.replica_config
    }
}

impl Configuration {
    /// Creates a new [Configuration] based on the provided [ReplicaConfig].
    ///
    /// This constructor also computes its [Fingerprint].
    pub fn new(
        storage_key_expr: OwnedKeyExpr,
        prefix: Option<OwnedKeyExpr>,
        replica_config: ReplicaConfig,
    ) -> Self {
        let mut hasher = xxhash_rust::xxh3::Xxh3::default();
        hasher.update(storage_key_expr.as_bytes());
        if let Some(prefix) = &prefix {
            hasher.update(prefix.as_bytes());
        }
        hasher.update(&replica_config.interval.as_millis().to_le_bytes());
        hasher.update(&replica_config.sub_intervals.to_le_bytes());
        hasher.update(&replica_config.hot.to_le_bytes());
        hasher.update(&replica_config.warm.to_le_bytes());
        hasher.update(&replica_config.propagation_delay.as_millis().to_le_bytes());

        Self {
            storage_key_expr,
            prefix,
            replica_config,
            fingerprint: Fingerprint::from(hasher.digest()),
        }
    }

    /// Returns the `prefix`, if one is set, that is stripped before keys are stored in the Storage.
    ///
    /// This corresponds to the `strip_prefix` configuration parameter of the Storage.
    ///
    /// TODO Rename this field and method to `strip_prefix` for consistency.
    pub fn prefix(&self) -> Option<&OwnedKeyExpr> {
        self.prefix.as_ref()
    }

    /// Returns the [Fingerprint] of the `Configuration`.
    ///
    /// The fingerprint is the hash of all its fields, using the `xxhash_rust` crate.
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Returns the last elapsed [Interval].
    ///
    /// This method will call [SystemTime::now()] to get the current timestamp and, based on the
    /// `interval` configuration, return the last [Interval] that elapsed.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The computation to obtain the duration elapsed since `UNIX_EPOCH` failed. Unless the
    ///   internal clock of the host is set to a time earlier than `UNIX_EPOCH`, this should never
    ///   happen.
    /// - The index of the current interval is higher than `u64::MAX`. Again, unless the internal
    ///   clock of the host machine is set to a time (very) far in the future, this should never
    ///   happen.
    ///
    /// ⚠️ **Both errors cannot be recovered from**.
    ///
    /// [Interval]: super::classification::Interval
    pub fn last_elapsed_interval(&self) -> Result<IntervalIdx> {
        let duration_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)?;

        let last_elapsed_interval = duration_since_epoch.as_millis() / self.interval.as_millis();

        if last_elapsed_interval > u64::MAX as u128 {
            bail!("Overflow detected, last elapsed interval is higher than u64::MAX");
        }

        Ok(IntervalIdx(last_elapsed_interval as u64))
    }

    /// Returns the index of the lowest interval contained in the *Hot* Era, assuming that the
    /// highest interval contained in the *Hot* Era is the one provided.
    ///
    /// # Example
    ///
    /// ```no_compile
    /// use crate::replication::Configuration;
    ///
    /// // If we assume the following configuration:
    /// let mut replica_config = ReplicaConfig::default();
    /// replica_config.hot = 2;
    ///
    /// let configuration = Configuration::new(replica_config);
    /// configuration.hot_era_lower_bound(IntervalIdx(10)); // IntervalIdx(9)
    /// ```
    pub fn hot_era_lower_bound(&self, hot_era_upper_bound: IntervalIdx) -> IntervalIdx {
        (*hot_era_upper_bound - self.hot + 1).into()
    }

    /// Returns the index of the lowest interval contained in the *Warm* Era, assuming that the
    /// highest interval contained in the *Hot* Era is the one provided.
    ///
    /// ⚠️ Note that, even though this method computes the lower bound of the WARM era, the index
    /// provided is the upper bound of the HOT era.
    ///
    /// # Example
    ///
    /// ```no_compile
    /// use crate::replication::Configuration;
    ///
    /// // If we assume the following configuration:
    /// let mut replica_config = ReplicaConfig::default();
    /// replica_config.hot = 2;
    /// replica_config.warm = 5;
    ///
    /// let configuration = Configuration::new(replica_config);
    /// configuration.warm_era_lower_bound(IntervalIdx(10)); // IntervalIdx(4)
    /// ```
    pub fn warm_era_lower_bound(&self, hot_era_upper_bound: IntervalIdx) -> IntervalIdx {
        (*hot_era_upper_bound - self.hot - self.warm + 1).into()
    }

    /// Returns the time classification — i.e. [Interval] and [SubInterval] — of the provided
    /// [Timestamp].
    ///
    /// # Errors
    ///
    /// This method will return an error in the following cases:
    /// - The call to compute the duration since [UNIX_EPOCH] for the provided [Timestamp] returned
    ///   an inconsistent value. This can happen only if the provided [Timestamp] is earlier than
    ///   [UNIX_EPOCH], which, in normal circumstances, should not happen.
    /// - The [Interval] associated with the provided [Timestamp] is higher than [u64::MAX]. This
    ///   can only happen if the [Timestamp] is (very) far in the future.
    ///
    /// The first error is not recoverable but would also be triggered by calls to the method
    /// `get_last_elapsed_interval()` – which is called regularly and will effectively stop the
    /// replication.
    ///
    /// The second error is recoverable: it is entirely possible to receive a publication with an
    /// erroneous timestamp (willingly or unwillingly).
    ///
    /// Hence, errors resulting from this call can be safely logged and ignored.
    ///
    /// [Interval]: super::classification::Interval
    /// [SubInterval]: super::classification::SubInterval
    pub fn get_time_classification(
        &self,
        timestamp: &Timestamp,
    ) -> Result<(IntervalIdx, SubIntervalIdx)> {
        let timestamp_ms_since_epoch = timestamp
            .get_time()
            .to_system_time()
            .duration_since(UNIX_EPOCH)?
            .as_millis();

        let interval = timestamp_ms_since_epoch / self.interval.as_millis();
        if interval > u64::MAX as u128 {
            bail!(
                "Overflow detected, interval associated with Timestamp < {} > is higher than \
                 u64::MAX",
                timestamp.to_string()
            );
        }

        let sub_interval = (timestamp_ms_since_epoch - (self.interval.as_millis() * interval))
            / (self.interval.as_millis() / self.sub_intervals as u128);
        let interval = interval as u64;

        Ok((
            IntervalIdx::from(interval),
            SubIntervalIdx::from(sub_interval as u64),
        ))
    }
}

#[cfg(test)]
#[path = "tests/configuration.test.rs"]
mod test;
