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

use super::{Digest, EraType};
use super::{LogEntry, Snapshotter};
use async_std::sync::Arc;
use async_std::sync::RwLock;
use flume::{Receiver, Sender};
use log::{error, trace};
use std::collections::{HashMap, HashSet};
use std::str;
use zenoh::key_expr::{KeyExpr, OwnedKeyExpr};
use zenoh::prelude::r#async::*;
use zenoh::query::QueryConsolidation;
use zenoh::time::Timestamp;
use zenoh::Session;

pub struct Aligner {
    session: Arc<Session>,
    digest_key: OwnedKeyExpr,
    snapshotter: Arc<Snapshotter>,
    rx_digest: Receiver<(String, super::Digest)>,
    tx_sample: Sender<Sample>,
    digests_processed: RwLock<HashSet<u64>>,
}

impl Aligner {
    pub async fn start_aligner(
        session: Arc<Session>,
        digest_key: OwnedKeyExpr,
        rx_digest: Receiver<(String, super::Digest)>,
        tx_sample: Sender<Sample>,
        snapshotter: Arc<Snapshotter>,
    ) {
        let aligner = Aligner {
            session,
            digest_key,
            snapshotter,
            rx_digest,
            tx_sample,
            digests_processed: RwLock::new(HashSet::new()),
        };
        aligner.start().await;
    }

    pub async fn start(&self) {
        while let Ok((from, incoming_digest)) = self.rx_digest.recv_async().await {
            if self.in_processed(incoming_digest.checksum).await {
                trace!(
                    "[ALIGNER]Skipping already processed digest: {}",
                    incoming_digest.checksum
                );
                continue;
            } else if self.snapshotter.get_digest().await.checksum == incoming_digest.checksum {
                trace!(
                    "[ALIGNER]Skipping matching digest: {}",
                    incoming_digest.checksum
                );
                continue;
            } else {
                // process this digest
                trace!(
                    "[ALIGNER]Processing digest: {:?} from {}",
                    incoming_digest,
                    from
                );
                self.process_incoming_digest(incoming_digest, &from).await;
            }
        }
    }

    async fn in_processed(&self, checksum: u64) -> bool {
        let processed_set = self.digests_processed.read().await;
        processed_set.contains(&checksum)
    }

    //identify alignment requirements
    async fn process_incoming_digest(&self, other: super::Digest, from: &str) {
        let checksum = other.checksum;
        let timestamp = other.timestamp;
        let missing_content = self.get_missing_content(&other, from).await;
        trace!("[ALIGNER] Missing content is {:?}", missing_content);

        // If missing content is not identified, it showcases some problem
        // The problem will be addressed in the future rounds, hence will not count as processed
        if !missing_content.is_empty() {
            let missing_data = self
                .get_missing_data(&missing_content, timestamp, from)
                .await;

            // Missing data might be empty since some samples in digest might be outdated
            trace!("[ALIGNER] Missing data is {:?}", missing_data);

            for (key, (ts, value)) in missing_data {
                let sample = Sample::new(key, value).with_timestamp(ts);
                trace!("[ALIGNER] Adding sample {:?} to storage", sample);
                match self.tx_sample.send_async(sample).await {
                    Ok(()) => continue,
                    Err(e) => error!("[ALIGNER] Error adding sample to storage: {}", e),
                }
            }
            let mut processed = self.digests_processed.write().await;
            (*processed).insert(checksum);
            drop(processed);
        }
    }

    async fn get_missing_data(
        &self,
        missing_content: &[LogEntry],
        timestamp: Timestamp,
        from: &str,
    ) -> HashMap<OwnedKeyExpr, (Timestamp, Value)> {
        let mut result = HashMap::new();
        let properties = format!(
            "timestamp={}&{}={}",
            timestamp,
            super::CONTENTS,
            serde_json::to_string(missing_content).unwrap()
        );
        let replies = self
            .perform_query(from.to_string(), properties.clone())
            .await;

        for sample in replies {
            result.insert(
                sample.key_expr.into(),
                (sample.timestamp.unwrap(), sample.value),
            );
        }
        result
    }

    async fn get_missing_content(&self, other: &super::Digest, from: &str) -> Vec<LogEntry> {
        // get my digest
        let this = &self.snapshotter.get_digest().await;

        let cold_alignment =
            self.perform_era_alignment(&EraType::Cold, this, from.to_string(), other);
        let warm_alignment =
            self.perform_era_alignment(&EraType::Warm, this, from.to_string(), other);
        let hot_alignment =
            self.perform_era_alignment(&EraType::Hot, this, from.to_string(), other);

        let (cold_data, warm_data, hot_data) =
            futures::join!(cold_alignment, warm_alignment, hot_alignment);
        [cold_data, warm_data, hot_data].concat()
    }

    //perform cold alignment
    // if COLD misaligned, ask for interval hashes for cold for the other digest timestamp and replica
    // for misaligned intervals, ask subinterval hashes
    // for misaligned subintervals, ask content
    async fn perform_era_alignment(
        &self,
        era: &EraType,
        this: &super::Digest,
        other_rep: String,
        other: &super::Digest,
    ) -> Vec<LogEntry> {
        if !this.era_has_diff(era, &other.eras) {
            return Vec::new();
        }
        // get era diff
        let diff_intervals = self.get_interval_diff(era, this, other, &other_rep).await;
        // get interval diff
        let diff_subintervals = self
            .get_subinterval_diff(era, diff_intervals, this, other, &other_rep)
            .await;
        // get subinterval diff
        self.get_content_diff(diff_subintervals, this, other, &other_rep)
            .await
    }

    async fn get_interval_diff(
        &self,
        era: &EraType,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> HashSet<u64> {
        let other_intervals = if era.eq(&EraType::Cold) {
            let properties = format!("timestamp={}&{}=cold", other.timestamp, super::ERA);
            // expecting sample.value to be a vec of intervals with their checksum
            let reply_content = self.perform_query(other_rep.to_string(), properties).await;
            let mut other_intervals: HashMap<u64, u64> = HashMap::new();
            for each in reply_content {
                match serde_json::from_str(&each.value.to_string()) {
                    Ok((i, c)) => {
                        other_intervals.insert(i, c);
                    }
                    Err(e) => error!("[ALIGNER] Error decoding reply: {}", e),
                };
            }
            // get era diff
            other_intervals
        } else {
            // get the diff of intervals from the digest itself
            // get interval hashes for WARM intervals from other
            other.get_era_content(era)
        };
        // get era diff
        this.get_interval_diff(other_intervals)
    }

    async fn get_subinterval_diff(
        &self,
        era: &EraType,
        diff_intervals: HashSet<u64>,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> HashSet<u64> {
        if !diff_intervals.is_empty() {
            let other_subintervals = if era.eq(&EraType::Hot) {
                // get subintervals for mismatching intervals from other
                other.get_interval_content(diff_intervals)
            } else {
                let mut diff_string = Vec::new();
                for each_int in diff_intervals {
                    diff_string.push(each_int.to_string());
                }
                let properties = format!(
                    "timestamp={}&{}=[{}]",
                    other.timestamp,
                    super::INTERVALS,
                    diff_string.join(",")
                );
                // expecting sample.value to be a vec of subintervals with their checksum
                let reply_content = self.perform_query(other_rep.to_string(), properties).await;
                let mut other_subintervals: HashMap<u64, u64> = HashMap::new();
                for each in reply_content {
                    match serde_json::from_str(&each.value.to_string()) {
                        Ok((i, c)) => {
                            other_subintervals.insert(i, c);
                        }
                        Err(e) => error!("[ALIGNER] Error decoding reply: {}", e),
                    };
                }
                other_subintervals
            };
            // get intervals diff
            this.get_subinterval_diff(other_subintervals)
        } else {
            HashSet::new()
        }
    }

    async fn get_content_diff(
        &self,
        diff_subintervals: HashSet<u64>,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> Vec<LogEntry> {
        if !diff_subintervals.is_empty() {
            let mut diff_string = Vec::new();
            for each_sub in diff_subintervals {
                diff_string.push(each_sub.to_string());
            }
            let properties = format!(
                "timestamp={}&{}=[{}]",
                other.timestamp,
                super::SUBINTERVALS,
                diff_string.join(",")
            );
            // expecting sample.value to be a vec of log entries with their checksum
            let reply_content = self.perform_query(other_rep.to_string(), properties).await;
            let mut other_content: HashMap<u64, Vec<LogEntry>> = HashMap::new();
            for each in reply_content {
                match serde_json::from_str(&each.value.to_string()) {
                    Ok((i, c)) => {
                        other_content.insert(i, c);
                    }
                    Err(e) => error!("[ALIGNER] Error decoding reply: {}", e),
                };
            }
            // get subintervals diff
            let result = this.get_full_content_diff(other_content);
            return result;
        }
        Vec::new()
    }

    async fn perform_query(&self, from: String, properties: String) -> Vec<Sample> {
        let selector = KeyExpr::from(&self.digest_key)
            .join(&from)
            .unwrap()
            .with_parameters(&properties);
        trace!("[ALIGNER]Sending Query '{}'...", selector);
        let mut return_val = Vec::new();
        let replies = self
            .session
            .get(&selector)
            .consolidation(QueryConsolidation::AUTO)
            .accept_replies(zenoh::query::ReplyKeyExpr::Any)
            .res()
            .await
            .unwrap();
        while let Ok(reply) = replies.recv_async().await {
            match reply.sample {
                Ok(sample) => {
                    trace!(
                        "[ALIGNER] Received ('{}': '{}')",
                        sample.key_expr.as_str(),
                        sample.value
                    );
                    return_val.push(sample);
                }
                Err(err) => error!("[ALIGNER] Query failed on selector {} :{}", selector, err),
            }
        }
        return_val
    }
}
