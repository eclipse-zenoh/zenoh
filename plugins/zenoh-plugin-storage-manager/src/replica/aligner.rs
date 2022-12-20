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
            trace!(
                "[ALIGNER]Processing digest: {:?} from {}",
                incoming_digest,
                from
            );
            if self.in_processed(incoming_digest.checksum).await {
                trace!("[ALIGNER]Skipping already processed digest");
                continue;
            } else if self.snapshotter.get_digest().await.checksum == incoming_digest.checksum {
                trace!("[ALIGNER]Skipping matching digest");
                continue;
            } else {
                // process this digest
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
        let missing_content = self.get_missing_content(other, from).await;
        trace!("[REPLICA] Missing content is {:?}", missing_content);

        if !missing_content.is_empty() {
            let missing_data = self
                .get_missing_data(&missing_content, timestamp, from)
                .await;

            trace!("[REPLICA] Missing data is {:?}", missing_data);

            for (key, (ts, value)) in missing_data {
                let sample = Sample::new(key, value).with_timestamp(ts);
                trace!("[REPLICA] Adding sample {:?} to storage", sample);
                match self.tx_sample.send_async(sample).await {
                    Ok(()) => continue,
                    Err(e) => error!("Error adding sample to storage: {}", e),
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

    async fn get_missing_content(&self, other: super::Digest, from: &str) -> Vec<LogEntry> {
        // get my digest
        let this = &self.snapshotter.get_digest().await;

        // get first level diff of digest wrt other - subintervals, of HOT, intervals of WARM and COLD if misaligned
        let mis_eras = this.get_era_diff(other.eras.clone());
        let mut missing_content = Vec::new();
        if mis_eras.contains(&super::EraType::Cold) {
            // perform cold alignment
            let mut cold_data = self
                .perform_cold_alignment(this, from.to_string(), other.timestamp)
                .await;
            missing_content.append(&mut cold_data);
        }
        if mis_eras.contains(&super::EraType::Warm) {
            // perform warm alignment
            let mut warm_data = self
                .perform_warm_alignment(this, from.to_string(), other.clone())
                .await;
            missing_content.append(&mut warm_data);
        }
        if mis_eras.contains(&super::EraType::Hot) {
            // perform hot alignment
            let mut hot_data = self
                .perform_hot_alignment(this, from.to_string(), other)
                .await;
            missing_content.append(&mut hot_data);
        }
        missing_content.into_iter().collect()
    }

    //perform cold alignment
    // if COLD misaligned, ask for interval hashes for cold for the other digest timestamp and replica
    // for misaligned intervals, ask subinterval hashes
    // for misaligned subintervals, ask content
    async fn perform_cold_alignment(
        &self,
        this: &super::Digest,
        other_rep: String,
        timestamp: Timestamp,
    ) -> Vec<LogEntry> {
        let properties = format!("timestamp={}&{}=cold", timestamp, super::ERA);
        // expecting sample.value to be a vec of intervals with their checksum
        let reply_content = self.perform_query(other_rep.to_string(), properties).await;
        let mut other_intervals: HashMap<u64, u64> = HashMap::new();
        for each in reply_content {
            match serde_json::from_str(&each.value.to_string()) {
                Ok((i, c)) => {
                    other_intervals.insert(i, c);
                }
                Err(e) => error!("Error decoding reply: {}", e),
            };
        }
        // get era diff
        let diff_intervals = this.get_interval_diff(other_intervals.clone());
        if !diff_intervals.is_empty() {
            let mut diff_string = Vec::new();
            for each_int in diff_intervals {
                diff_string.push(each_int.to_string());
            }
            let properties = format!(
                "timestamp={}&{}=[{}]",
                timestamp,
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
                    Err(e) => error!("Error decoding reply: {}", e),
                };
            }
            // get intervals diff
            let diff_subintervals = this.get_subinterval_diff(other_subintervals);
            trace!(
                "[ALIGNER] The subintervals that need alignment are : {:?}",
                diff_subintervals
            );
            if !diff_subintervals.is_empty() {
                let mut diff_string = Vec::new();
                for each_sub in diff_subintervals {
                    diff_string.push(each_sub.to_string());
                }
                let properties = format!(
                    "timestamp={}&{}=[{}]",
                    timestamp,
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
                        Err(e) => error!("Error decoding reply: {}", e),
                    };
                }
                // get subintervals diff
                let result = this.get_full_content_diff(other_content);
                trace!("[ALIGNER] The missing content is {:?}", result);
                return result;
            }
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
                Err(err) => error!("Query failed on selector '{}' ::{}", selector, err),
            }
        }
        return_val
    }

    //peform warm alignment
    // if WARM misaligned, ask for subinterval hashes of misaligned intervals
    // for misaligned subintervals, ask content
    async fn perform_warm_alignment(
        &self,
        this: &super::Digest,
        other_rep: String,
        other: super::Digest,
    ) -> Vec<LogEntry> {
        // get interval hashes for WARM intervals from other
        let other_intervals = other.get_era_content(super::EraType::Warm);
        // get era diff
        let diff_intervals = this.get_interval_diff(other_intervals);

        // properties = timestamp=xxx,intervals=[www,ee,rr]
        if !diff_intervals.is_empty() {
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
                    Err(e) => error!("Error decoding reply: {}", e),
                };
            }
            // get intervals diff
            let diff_subintervals = this.get_subinterval_diff(other_subintervals);
            if !diff_subintervals.is_empty() {
                let mut diff_string = Vec::new();
                // properties = timestamp=xxx,subintervals=[www,ee,rr]
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
                        Err(e) => error!("Error decoding reply: {}", e),
                    };
                }
                // get subintervals diff
                return this.get_full_content_diff(other_content);
            }
        }
        Vec::new()
    }

    //perform hot alignment
    // if HOT misaligned, ask for content(timestamps) of misaligned subintervals
    async fn perform_hot_alignment(
        &self,
        this: &super::Digest,
        other_rep: String,
        other: super::Digest,
    ) -> Vec<LogEntry> {
        // get interval hashes for HOT intervals from other
        let other_intervals = other.get_era_content(super::EraType::Hot);
        // get era diff
        let diff_intervals = this.get_interval_diff(other_intervals);

        // get subintervals for mismatching intervals from other
        let other_subintervals = other.get_interval_content(diff_intervals);
        // get intervals diff
        let diff_subintervals = this.get_subinterval_diff(other_subintervals);

        if !diff_subintervals.is_empty() {
            let mut diff_string = Vec::new();
            // properties = timestamp=xxx,subintervals=[www,ee,rr]
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
                    Err(e) => error!("Error decoding reply: {}", e),
                };
            }
            // get subintervals diff
            return this.get_full_content_diff(other_content);
        }
        Vec::new()
    }
}
