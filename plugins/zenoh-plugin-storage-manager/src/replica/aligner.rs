//
// Copyright (c) 2023 ZettaScale Technology
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

use super::{Digest, EraType, LogEntry, Snapshotter};
use super::{CONTENTS, ERA, INTERVALS, SUBINTERVALS};
use async_std::sync::{Arc, RwLock};
use flume::{Receiver, Sender};
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
    rx_digest: Receiver<(String, Digest)>,
    tx_sample: Sender<Sample>,
    digests_processed: RwLock<HashSet<u64>>,
}

impl Aligner {
    pub async fn start_aligner(
        session: Arc<Session>,
        digest_key: OwnedKeyExpr,
        rx_digest: Receiver<(String, Digest)>,
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
                log::trace!(
                    "[ALIGNER]Skipping already processed digest: {}",
                    incoming_digest.checksum
                );
                continue;
            } else if self.snapshotter.get_digest().await.checksum == incoming_digest.checksum {
                log::trace!(
                    "[ALIGNER]Skipping matching digest: {}",
                    incoming_digest.checksum
                );
                continue;
            } else {
                // process this digest
                log::debug!(
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
    async fn process_incoming_digest(&self, other: Digest, from: &str) {
        let checksum = other.checksum;
        let timestamp = other.timestamp;
        let (missing_content, no_content_err) = self.get_missing_content(&other, from).await;
        log::trace!("[ALIGNER] Missing content is {:?}", missing_content);

        // If missing content is not identified, it showcases some problem
        // The problem will be addressed in the future rounds, hence will not count as processed
        if !missing_content.is_empty() {
            let (missing_data, no_data_err) = self
                .get_missing_data(&missing_content, timestamp, from)
                .await;

            // Missing data might be empty since some samples in digest might be outdated
            log::trace!("[ALIGNER] Missing data is {:?}", missing_data);

            for (key, (ts, value)) in missing_data {
                let sample = Sample::new(key, value).with_timestamp(ts);
                log::debug!("[ALIGNER] Adding sample {:?} to storage", sample);
                self.tx_sample.send_async(sample).await.unwrap_or_else(|e| {
                    log::error!("[ALIGNER] Error adding sample to storage: {}", e)
                });
            }

            if no_content_err && no_data_err {
                let mut processed = self.digests_processed.write().await;
                (*processed).insert(checksum);
            }
        }
    }

    async fn get_missing_data(
        &self,
        missing_content: &[LogEntry],
        timestamp: Timestamp,
        from: &str,
    ) -> (HashMap<OwnedKeyExpr, (Timestamp, Value)>, bool) {
        let mut result = HashMap::new();
        let properties = format!(
            "timestamp={}&{}={}",
            timestamp,
            CONTENTS,
            serde_json::to_string(missing_content).unwrap()
        );
        let (replies, no_err) = self.perform_query(from, properties.clone()).await;

        for sample in replies {
            result.insert(
                sample.key_expr.into(),
                (sample.timestamp.unwrap(), sample.value),
            );
        }
        (result, no_err)
    }

    async fn get_missing_content(&self, other: &Digest, from: &str) -> (Vec<LogEntry>, bool) {
        // get my digest
        let this = &self.snapshotter.get_digest().await;

        let cold_alignment =
            self.perform_era_alignment(&EraType::Cold, this, from.to_string(), other);
        let warm_alignment =
            self.perform_era_alignment(&EraType::Warm, this, from.to_string(), other);
        let hot_alignment =
            self.perform_era_alignment(&EraType::Hot, this, from.to_string(), other);

        let ((cold_data, no_cold_err), (warm_data, no_warm_err), (hot_data, no_hot_err)) =
            futures::join!(cold_alignment, warm_alignment, hot_alignment);
        (
            [cold_data, warm_data, hot_data].concat(),
            no_cold_err && no_warm_err && no_hot_err,
        )
    }

    // perform era alignment
    // for a misaligned era, get missing intervals, then subintervals and then log entry
    async fn perform_era_alignment(
        &self,
        era: &EraType,
        this: &Digest,
        other_rep: String,
        other: &Digest,
    ) -> (Vec<LogEntry>, bool) {
        if !this.era_has_diff(era, &other.eras) {
            return (Vec::new(), true);
        }
        // get era diff
        let (diff_intervals, no_era_err) =
            self.get_interval_diff(era, this, other, &other_rep).await;
        // get interval diff
        let (diff_subintervals, no_int_err) = self
            .get_subinterval_diff(era, diff_intervals, this, other, &other_rep)
            .await;
        // get subinterval diff
        let (diff_content, no_sub_err) = self
            .get_content_diff(diff_subintervals, this, other, &other_rep)
            .await;
        (diff_content, no_era_err && no_int_err && no_sub_err)
    }

    async fn get_interval_diff(
        &self,
        era: &EraType,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> (HashSet<u64>, bool) {
        let (other_intervals, no_err) = if era.eq(&EraType::Cold) {
            let properties = format!("timestamp={}&{}=cold", other.timestamp, ERA);
            let (reply_content, mut no_err) = self.perform_query(other_rep, properties).await;
            let mut other_intervals: HashMap<u64, u64> = HashMap::new();
            // expecting sample.value to be a vec of intervals with their checksum
            for each in reply_content {
                match serde_json::from_str(&each.value.to_string()) {
                    Ok((i, c)) => {
                        other_intervals.insert(i, c);
                    }
                    Err(e) => {
                        log::error!("[ALIGNER] Error decoding reply: {}", e);
                        no_err = false;
                    }
                };
            }
            (other_intervals, no_err)
        } else {
            // get the diff of intervals from the digest itself
            (other.get_era_content(era), true)
        };
        // get era diff
        (this.get_interval_diff(other_intervals), no_err)
    }

    async fn get_subinterval_diff(
        &self,
        era: &EraType,
        diff_intervals: HashSet<u64>,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> (HashSet<u64>, bool) {
        if !diff_intervals.is_empty() {
            let (other_subintervals, no_err) = if era.eq(&EraType::Hot) {
                // get subintervals for mismatching intervals from other
                (other.get_interval_content(diff_intervals), true)
            } else {
                let mut diff_string = Vec::new();
                for each_int in diff_intervals {
                    diff_string.push(each_int.to_string());
                }
                let properties = format!(
                    "timestamp={}&{}=[{}]",
                    other.timestamp,
                    INTERVALS,
                    diff_string.join(",")
                );
                // expecting sample.value to be a vec of subintervals with their checksum
                let (reply_content, mut no_err) = self.perform_query(other_rep, properties).await;
                let mut other_subintervals: HashMap<u64, u64> = HashMap::new();
                for each in reply_content {
                    match serde_json::from_str(&each.value.to_string()) {
                        Ok((i, c)) => {
                            other_subintervals.insert(i, c);
                        }
                        Err(e) => {
                            log::error!("[ALIGNER] Error decoding reply: {}", e);
                            no_err = false;
                        }
                    };
                }
                (other_subintervals, no_err)
            };
            // get intervals diff
            (this.get_subinterval_diff(other_subintervals), no_err)
        } else {
            (HashSet::new(), true)
        }
    }

    async fn get_content_diff(
        &self,
        diff_subintervals: HashSet<u64>,
        this: &Digest,
        other: &Digest,
        other_rep: &str,
    ) -> (Vec<LogEntry>, bool) {
        if !diff_subintervals.is_empty() {
            let mut diff_string = Vec::new();
            for each_sub in diff_subintervals {
                diff_string.push(each_sub.to_string());
            }
            let properties = format!(
                "timestamp={}&{}=[{}]",
                other.timestamp,
                SUBINTERVALS,
                diff_string.join(",")
            );
            // expecting sample.value to be a vec of log entries with their checksum
            let (reply_content, mut no_err) = self.perform_query(other_rep, properties).await;
            let mut other_content: HashMap<u64, Vec<LogEntry>> = HashMap::new();
            for each in reply_content {
                match serde_json::from_str(&each.value.to_string()) {
                    Ok((i, c)) => {
                        other_content.insert(i, c);
                    }
                    Err(e) => {
                        log::error!("[ALIGNER] Error decoding reply: {}", e);
                        no_err = false;
                    }
                };
            }
            // get subintervals diff
            let result = this.get_full_content_diff(other_content);
            (result, no_err)
        } else {
            (Vec::new(), true)
        }
    }

    async fn perform_query(&self, from: &str, properties: String) -> (Vec<Sample>, bool) {
        let mut no_err = true;
        let selector = KeyExpr::from(&self.digest_key)
            .join(&from)
            .unwrap()
            .with_parameters(&properties);
        log::trace!("[ALIGNER] Sending Query '{}'...", selector);
        let mut return_val = Vec::new();
        match self
            .session
            .get(&selector)
            .consolidation(QueryConsolidation::AUTO)
            .accept_replies(zenoh::query::ReplyKeyExpr::Any)
            .res()
            .await
        {
            Ok(replies) => {
                while let Ok(reply) = replies.recv_async().await {
                    match reply.sample {
                        Ok(sample) => {
                            log::trace!(
                                "[ALIGNER] Received ('{}': '{}')",
                                sample.key_expr.as_str(),
                                sample.value
                            );
                            return_val.push(sample);
                        }
                        Err(err) => {
                            log::error!(
                                "[ALIGNER] Received error for query on selector {} :{}",
                                selector,
                                err
                            );
                            no_err = false;
                        }
                    }
                }
            }
            Err(err) => {
                log::error!("[ALIGNER] Query failed on selector `{}`: {}", selector, err);
                no_err = false;
            }
        };
        (return_val, no_err)
    }
}
