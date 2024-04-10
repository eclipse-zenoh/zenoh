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
use super::digest::*;
use super::Snapshotter;
use async_std::sync::Arc;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::str;
use std::str::FromStr;
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh::Session;

pub struct AlignQueryable {
    session: Arc<Session>,
    digest_key: OwnedKeyExpr,
    snapshotter: Arc<Snapshotter>,
}

#[derive(Debug)]
enum AlignComponent {
    Era(EraType),
    Intervals(Vec<u64>),
    Subintervals(Vec<u64>),
    Contents(Vec<LogEntry>),
}
#[derive(Debug)]
enum AlignData {
    Interval(u64, u64),
    Subinterval(u64, u64),
    Content(u64, BTreeSet<LogEntry>),
    Data(OwnedKeyExpr, (Value, Timestamp)),
}

impl AlignQueryable {
    pub async fn start_align_queryable(
        session: Arc<Session>,
        digest_key: OwnedKeyExpr,
        replica_name: &str,
        snapshotter: Arc<Snapshotter>,
    ) {
        let digest_key = digest_key.join(replica_name).unwrap().join("**").unwrap();

        let align_queryable = AlignQueryable {
            session,
            digest_key,
            snapshotter,
        };

        align_queryable.start().await;
    }

    async fn start(&self) -> Self {
        log::debug!(
            "[ALIGN QUERYABLE] Declaring Queryable on '{}'...",
            self.digest_key
        );
        let queryable = self
            .session
            .declare_queryable(&self.digest_key)
            .complete(true) // This queryable is meant to have all the history
            .res()
            .await
            .unwrap();

        loop {
            let query = match queryable.recv_async().await {
                Ok(query) => query,
                Err(e) => {
                    log::error!("Error in receiving query: {}", e);
                    continue;
                }
            };
            log::trace!("[ALIGN QUERYABLE] Received Query '{}'", query.selector());
            let diff_required = self.parse_selector(query.selector());
            log::trace!(
                "[ALIGN QUERYABLE] Parsed selector diff_required:{:?}",
                diff_required
            );
            if diff_required.is_some() {
                let values = self.get_value(diff_required.unwrap()).await;
                log::trace!("[ALIGN QUERYABLE] value for the query is {:?}", values);
                for value in values {
                    match value {
                        AlignData::Interval(i, c) => {
                            let sample = Sample::new(
                                query.key_expr().clone(),
                                serde_json::to_string(&(i, c)).unwrap(),
                            );
                            query.reply(Ok(sample)).res().await.unwrap();
                        }
                        AlignData::Subinterval(i, c) => {
                            let sample = Sample::new(
                                query.key_expr().clone(),
                                serde_json::to_string(&(i, c)).unwrap(),
                            );
                            query.reply(Ok(sample)).res().await.unwrap();
                        }
                        AlignData::Content(i, c) => {
                            let sample = Sample::new(
                                query.key_expr().clone(),
                                serde_json::to_string(&(i, c)).unwrap(),
                            );
                            query.reply(Ok(sample)).res().await.unwrap();
                        }
                        AlignData::Data(k, (v, ts)) => {
                            let sample = Sample::new(k, v).with_timestamp(ts);
                            query.reply(Ok(sample)).res().await.unwrap();
                        }
                    }
                }
            }
        }
    }

    async fn get_value(&self, diff_required: AlignComponent) -> Vec<AlignData> {
        // TODO: Discuss if having timestamp is useful
        match diff_required {
            AlignComponent::Era(era) => {
                let intervals = self.get_intervals(&era).await;
                let mut result = Vec::new();
                for (i, c) in intervals {
                    result.push(AlignData::Interval(i, c));
                }
                result
            }
            AlignComponent::Intervals(intervals) => {
                let mut subintervals = HashMap::new();
                for each in intervals {
                    subintervals.extend(self.get_subintervals(each).await);
                }
                let mut result = Vec::new();
                for (i, c) in subintervals {
                    result.push(AlignData::Subinterval(i, c));
                }
                result
            }
            AlignComponent::Subintervals(subintervals) => {
                let mut content = HashMap::new();
                for each in subintervals {
                    content.extend(self.get_content(each).await);
                }
                let mut result = Vec::new();
                for (i, c) in content {
                    result.push(AlignData::Content(i, c));
                }
                result
            }
            AlignComponent::Contents(contents) => {
                let mut result = Vec::new();
                for each in contents {
                    let entry = self.get_entry(&each).await;
                    if entry.is_some() {
                        let entry = entry.unwrap();
                        result.push(AlignData::Data(
                            OwnedKeyExpr::from(entry.key_expr),
                            (entry.value, each.timestamp),
                        ));
                    }
                }
                result
            }
        }
    }

    fn parse_selector(&self, selector: Selector) -> Option<AlignComponent> {
        let properties = selector.parameters_stringmap().unwrap(); // note: this is a hashmap
        log::trace!("[ALIGN QUERYABLE] Properties are: {:?}", properties);
        if properties.get(super::ERA).is_some() {
            Some(AlignComponent::Era(
                EraType::from_str(properties.get(super::ERA).unwrap()).unwrap(),
            ))
        } else if properties.get(super::INTERVALS).is_some() {
            let mut intervals = properties.get(super::INTERVALS).unwrap().to_string();
            intervals.remove(0);
            intervals.pop();
            Some(AlignComponent::Intervals(
                intervals
                    .split(',')
                    .map(|x| x.parse::<u64>().unwrap())
                    .collect::<Vec<u64>>(),
            ))
        } else if properties.get(super::SUBINTERVALS).is_some() {
            let mut subintervals = properties.get(super::SUBINTERVALS).unwrap().to_string();
            subintervals.remove(0);
            subintervals.pop();
            Some(AlignComponent::Subintervals(
                subintervals
                    .split(',')
                    .map(|x| x.parse::<u64>().unwrap())
                    .collect::<Vec<u64>>(),
            ))
        } else if properties.get(super::CONTENTS).is_some() {
            let contents = serde_json::from_str(properties.get(super::CONTENTS).unwrap()).unwrap();
            Some(AlignComponent::Contents(contents))
        } else {
            None
        }
    }
}

// replying queries
impl AlignQueryable {
    async fn get_entry(&self, logentry: &LogEntry) -> Option<Sample> {
        // get corresponding key from log
        let replies = self.session.get(&logentry.key).res().await.unwrap();
        if let Ok(reply) = replies.recv_async().await {
            match reply.sample {
                Ok(sample) => {
                    log::trace!(
                        "[ALIGN QUERYABLE] Received ('{}': '{}')",
                        sample.key_expr.as_str(),
                        sample.value
                    );
                    if let Some(timestamp) = sample.timestamp {
                        match timestamp.cmp(&logentry.timestamp) {
                            Ordering::Greater => return None,
                            Ordering::Less => {
                                log::error!(
                                    "[ALIGN QUERYABLE] Data in the storage is older than requested."
                                );
                                return None;
                            }
                            Ordering::Equal => return Some(sample),
                        }
                    }
                }
                Err(err) => {
                    log::error!(
                        "[ALIGN QUERYABLE] Error when requesting storage: {:?}.",
                        err
                    );
                    return None;
                }
            }
        }
        None
    }

    async fn get_intervals(&self, era: &EraType) -> HashMap<u64, u64> {
        let digest = self.snapshotter.get_digest().await;
        digest.get_era_content(era)
    }

    async fn get_subintervals(&self, interval: u64) -> HashMap<u64, u64> {
        let digest = self.snapshotter.get_digest().await;
        let mut intervals = HashSet::new();
        intervals.insert(interval);
        digest.get_interval_content(intervals)
    }

    async fn get_content(&self, subinterval: u64) -> HashMap<u64, BTreeSet<LogEntry>> {
        let digest = self.snapshotter.get_digest().await;
        let mut subintervals = HashSet::new();
        subintervals.insert(subinterval);
        digest.get_subinterval_content(subintervals)
    }
}
