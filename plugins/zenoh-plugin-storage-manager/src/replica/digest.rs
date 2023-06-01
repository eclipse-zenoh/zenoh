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

use crc::{Crc, CRC_64_ECMA_182};
use derive_new::new;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::TryFrom;
use std::str::FromStr;
use std::string::ParseError;
use std::time::Duration;
use zenoh::key_expr::OwnedKeyExpr;
use zenoh::time::Timestamp;

#[derive(Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct DigestConfig {
    pub delta: Duration,
    pub sub_intervals: usize,
    pub hot: usize,
    pub warm: usize,
}

#[derive(Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct Digest {
    pub timestamp: Timestamp,
    pub config: DigestConfig,
    pub checksum: u64,
    pub eras: HashMap<EraType, Interval>,
    pub intervals: HashMap<u64, Interval>,
    pub subintervals: HashMap<u64, SubInterval>,
}

#[derive(new, Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct Interval {
    #[new(default)]
    pub checksum: u64,
    #[new(default)]
    pub content: BTreeSet<u64>,
}

#[derive(new, Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct SubInterval {
    #[new(default)]
    pub checksum: u64,
    #[new(default)]
    pub content: BTreeSet<LogEntry>,
}

#[derive(new, Eq, PartialEq, Clone, Debug, Deserialize, Serialize, Hash)]
pub struct LogEntry {
    pub timestamp: Timestamp,
    pub key: OwnedKeyExpr,
}

impl Ord for LogEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

impl PartialOrd for LogEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(PartialEq, Eq, Hash, Ord, PartialOrd, Debug, Clone, Deserialize, Serialize)]
pub enum EraType {
    Hot,
    Warm,
    Cold,
}

impl FromStr for EraType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, std::convert::Infallible> {
        let s = s.to_lowercase();
        match s.as_str() {
            "cold" => Ok(EraType::Cold),
            "warm" => Ok(EraType::Warm),
            "hot" => Ok(EraType::Hot),
            _ => Ok(EraType::Cold), //TODO: fix this later with proper error message
        }
    }
}

trait Checksum {
    fn format_content(&self) -> String;
}

impl Checksum for LogEntry {
    fn format_content(&self) -> String {
        format!("{}-{}", self.timestamp, self.key)
    }
}

impl Checksum for u64 {
    fn format_content(&self) -> String {
        self.to_string()
    }
}

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct EraParseError();

// impl Error for EraParseError {
//     fn description(&self) -> &str {
//         "invalid era"
//     }
// }

// impl fmt::Display for EraParseError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "invalid era")
//     }
// }

//functions for digest creation and update
impl Digest {
    // Creates a digest from scratch when initializing the replica
    pub fn create_digest(
        timestamp: Timestamp,
        config: DigestConfig,
        mut raw_log: Vec<LogEntry>,
        latest_interval: u64,
    ) -> Digest {
        // sort log
        // traverse through log
        // keep track of current subinterval, interval, era
        // when sub interval changes, compute the hash until then, save it for interval hash and move forward
        // when interval changes, compute the hash until then, save it for era hash and move forward
        // when era changes, compute the era hash, save it for digest hash and move forward

        raw_log.sort();
        let (mut eras, mut intervals, mut subintervals) =
            (HashMap::new(), HashMap::new(), HashMap::new());
        let (mut curr_sub, mut curr_int, mut curr_era) = (0, 0, EraType::Cold);
        let (mut sub_content, mut int_content, mut era_content, mut digest_hash) = (
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            HashMap::new(),
        );
        for entry in raw_log {
            let sub = Digest::get_subinterval(config.delta, entry.timestamp, config.sub_intervals);
            let interval = Digest::get_interval(sub, config.sub_intervals);
            let era = Digest::get_era(&config, latest_interval, interval);

            if sub == curr_sub {
                sub_content.insert(entry);
            } else {
                if !sub_content.is_empty() {
                    // compute checksum, store it for interval
                    let checksum = Digest::get_content_hash(
                        &sub_content.clone().into_iter().collect::<Vec<LogEntry>>(),
                    );
                    let s = SubInterval {
                        checksum,
                        content: sub_content,
                    };
                    subintervals.insert(curr_sub, s);
                    // update interval
                    int_content.insert(curr_sub, checksum);
                } // else, no action needed
                curr_sub = sub;
                sub_content = BTreeSet::new();
                sub_content.insert(entry);
                if interval != curr_int {
                    if !int_content.is_empty() {
                        let int_hash: Vec<u64> = int_content.values().copied().collect();
                        let checksum = Digest::get_content_hash(&int_hash);
                        let i = Interval {
                            checksum,
                            content: int_content.keys().copied().collect(),
                        };
                        intervals.insert(curr_int, i);
                        // update era
                        era_content.insert(curr_int, checksum);
                    }
                    curr_int = interval;
                    int_content = BTreeMap::new();
                    if era != curr_era {
                        if !era_content.is_empty() {
                            let era_hash: Vec<u64> = era_content.values().copied().collect();
                            let checksum = Digest::get_content_hash(&era_hash);
                            let e = Interval {
                                checksum,
                                content: era_content.keys().copied().collect(),
                            };
                            eras.insert(curr_era.clone(), e);
                            // update digest
                            digest_hash.insert(curr_era, checksum);
                        }
                        curr_era = era;
                        era_content = BTreeMap::new();
                    }
                }
            }
        }
        if !sub_content.is_empty() {
            // close subinterval
            let checksum = Digest::get_content_hash(
                &sub_content.clone().into_iter().collect::<Vec<LogEntry>>(),
            );
            let s = SubInterval {
                checksum,
                content: sub_content,
            };
            subintervals.insert(curr_sub, s);
            // update interval
            int_content.insert(curr_sub, checksum);
            let int_hash: Vec<u64> = int_content.values().copied().collect();
            let checksum = Digest::get_content_hash(&int_hash);
            let i = Interval {
                checksum,
                content: int_content.keys().copied().collect(),
            };
            intervals.insert(curr_int, i);
            // update era
            era_content.insert(curr_int, checksum);
            let era_hash: Vec<u64> = era_content.values().copied().collect();
            let checksum = Digest::get_content_hash(&era_hash);
            let e = Interval {
                checksum,
                content: era_content.keys().copied().collect(),
            };
            eras.insert(curr_era.clone(), e);
            digest_hash.insert(curr_era, checksum);
        }
        // update and compute digest
        let mut digest_content = Vec::new();
        if let Some(checksum) = digest_hash.get(&EraType::Cold) {
            digest_content.push(*checksum);
        }
        if let Some(checksum) = digest_hash.get(&EraType::Warm) {
            digest_content.push(*checksum);
        }
        if let Some(checksum) = digest_hash.get(&EraType::Hot) {
            digest_content.push(*checksum);
        }
        let checksum = Digest::get_content_hash(&digest_content);

        Digest {
            timestamp,
            config,
            checksum,
            eras,
            intervals,
            subintervals,
        }
    }

    // Updates an existing digest with new entries, also replaces removed entries
    pub async fn update_digest(
        mut current: Digest,
        latest_interval: u64,
        last_snapshot_time: Timestamp,
        new_content: HashSet<LogEntry>,
        deleted_content: HashSet<LogEntry>,
    ) -> Digest {
        // push content in correct places
        let (mut current, mut subintervals_to_update, mut intervals_to_update, mut eras_to_update) =
            Digest::update_new_content(&mut current, new_content, latest_interval);

        // remove deleted content from proper places
        let (mut current, further_subintervals, further_intervals, further_eras) =
            Digest::remove_deleted_content(&mut current, deleted_content, latest_interval);

        // move intervals into eras if changed -- iterate through hot and move them to warm/cold if needed, iterate through warm and move them to cold if needed
        let (current, realigned_eras) =
            Digest::recalculate_era_content(&mut current, latest_interval);

        subintervals_to_update.extend(further_subintervals);
        intervals_to_update.extend(further_intervals);
        eras_to_update.extend(further_eras);
        eras_to_update.extend(realigned_eras);

        let mut subintervals = current.subintervals.clone();
        let mut intervals = current.intervals.clone();
        let mut eras = current.eras.clone();

        // reconstruct updated parts of the digest
        for sub in subintervals_to_update {
            let mut subinterval = subintervals.get_mut(&sub).unwrap();
            let content = &subinterval.content;
            if !content.is_empty() {
                // order the content, hash them
                let checksum = Digest::get_subinterval_checksum(
                    &content.clone().into_iter().collect::<Vec<LogEntry>>(),
                );

                subinterval.checksum = checksum;
            } else {
                subintervals.remove(&sub);
            }
        }

        for int in intervals_to_update {
            let mut interval = intervals.get_mut(&int).unwrap();
            let content = &interval.content;
            if !content.is_empty() {
                // order the content, hash them
                let checksum = Digest::get_interval_checksum(
                    &content.clone().into_iter().collect::<Vec<u64>>(),
                    &subintervals,
                );

                interval.checksum = checksum;
            } else {
                intervals.remove(&int);
            }
        }

        for era_type in eras_to_update {
            let mut era = eras.get_mut(&era_type).unwrap();
            let content = &era.content;
            if !content.is_empty() {
                // order the content, hash them
                let checksum = Digest::get_era_checksum(
                    &content.iter().copied().collect::<Vec<u64>>(),
                    &intervals,
                );

                era.checksum = checksum;
            } else {
                eras.remove(&era_type);
            }
        }

        // update the shared value
        Digest {
            timestamp: last_snapshot_time,
            config: current.config,
            checksum: Digest::get_digest_checksum(&eras),
            eras,
            intervals,
            subintervals,
        }
    }

    // compute the checksum of the given content
    fn get_content_hash<T: Checksum>(content: &[T]) -> u64 {
        let crc64 = Crc::<u64>::new(&CRC_64_ECMA_182);
        let mut hasher = crc64.digest();
        for s_cont in content {
            let formatted = s_cont.format_content();
            hasher.update(formatted.as_bytes());
        }
        hasher.finalize()
    }

    // compute the checksum of a subinterval
    fn get_subinterval_checksum(content: &[LogEntry]) -> u64 {
        Digest::get_content_hash(content)
    }

    // compute the checksum of an interval
    fn get_interval_checksum(content: &[u64], info: &HashMap<u64, SubInterval>) -> u64 {
        let mut hashable_content = Vec::new();
        for i in content {
            let i_cont = info.get(i).unwrap().checksum;
            hashable_content.push(i_cont);
        }
        Digest::get_content_hash(&hashable_content)
    }

    // compute the checksum of an era
    fn get_era_checksum(content: &[u64], info: &HashMap<u64, Interval>) -> u64 {
        let mut hashable_content = Vec::new();
        for i in content {
            let i_cont = info.get(i).unwrap().checksum;
            hashable_content.push(i_cont);
        }
        Digest::get_content_hash(&hashable_content)
    }

    // compute the checksum of the digest
    fn get_digest_checksum(content: &HashMap<EraType, Interval>) -> u64 {
        let mut digest_content = Vec::new();
        if let Some(interval) = content.get(&EraType::Cold) {
            digest_content.push(interval.checksum);
        }
        if let Some(interval) = content.get(&EraType::Warm) {
            digest_content.push(interval.checksum);
        }
        if let Some(interval) = content.get(&EraType::Hot) {
            digest_content.push(interval.checksum);
        }
        Digest::get_content_hash(&digest_content)
    }

    // update the digest with new content
    fn update_new_content(
        current: &mut Digest,
        content: HashSet<LogEntry>,
        latest_interval: u64,
    ) -> (Digest, HashSet<u64>, HashSet<u64>, HashSet<EraType>) {
        let mut eras_to_update = HashSet::new();
        let mut intervals_to_update = HashSet::new();
        let mut subintervals_to_update = HashSet::new();

        for log_entry in content {
            let subinterval = Digest::get_subinterval(
                current.config.delta,
                log_entry.timestamp,
                current.config.sub_intervals,
            );
            subintervals_to_update.insert(subinterval);
            let interval = Digest::get_interval(subinterval, current.config.sub_intervals);
            intervals_to_update.insert(interval);
            let era = Digest::get_era(&current.config, latest_interval, interval);
            eras_to_update.insert(era.clone());

            current
                .subintervals
                .entry(subinterval)
                .and_modify(|e| {
                    e.content.insert(log_entry.clone());
                })
                .or_insert(SubInterval {
                    checksum: 0,
                    content: [log_entry].into(),
                });
            current
                .intervals
                .entry(interval)
                .and_modify(|e| {
                    e.content.insert(subinterval);
                })
                .or_insert(Interval {
                    checksum: 0,
                    content: [subinterval].into(),
                });
            current
                .eras
                .entry(era)
                .and_modify(|e| {
                    e.content.insert(interval);
                })
                .or_insert(Interval {
                    checksum: 0,
                    content: [interval].into(),
                });
        }

        (
            current.clone(),
            subintervals_to_update,
            intervals_to_update,
            eras_to_update,
        )
    }

    // remove deleted content from the digest
    fn remove_deleted_content(
        current: &mut Digest,
        deleted_content: HashSet<LogEntry>,
        latest_interval: u64,
    ) -> (Digest, HashSet<u64>, HashSet<u64>, HashSet<EraType>) {
        let mut eras_to_update = HashSet::new();
        let mut intervals_to_update = HashSet::new();
        let mut subintervals_to_update = HashSet::new();

        for entry in deleted_content {
            let subinterval = Digest::get_subinterval(
                current.config.delta,
                entry.timestamp,
                current.config.sub_intervals,
            );
            subintervals_to_update.insert(subinterval);
            let interval = Digest::get_interval(subinterval, current.config.sub_intervals);
            intervals_to_update.insert(interval);
            let era = Digest::get_era(&current.config, latest_interval, interval);
            eras_to_update.insert(era.clone());

            if current.subintervals.contains_key(&subinterval) {
                current
                    .subintervals
                    .get_mut(&subinterval)
                    .unwrap()
                    .content
                    .retain(|x| {
                        x.timestamp.get_time() != entry.timestamp.get_time()
                            && x.timestamp.get_id() != entry.timestamp.get_id()
                            && x.key != entry.key
                    });
                subintervals_to_update.insert(subinterval);
            }
            if current.intervals.contains_key(&interval) {
                current
                    .intervals
                    .get_mut(&interval)
                    .unwrap()
                    .content
                    .retain(|&x| x != subinterval);
                intervals_to_update.insert(interval);
            }
            if current.eras.contains_key(&era) {
                current
                    .eras
                    .get_mut(&era)
                    .unwrap()
                    .content
                    .retain(|&x| x != interval);
                eras_to_update.insert(era.clone());
            }
        }
        (
            current.clone(),
            subintervals_to_update,
            intervals_to_update,
            eras_to_update,
        )
    }

    // re-assign intervals into eras as time moves on
    fn recalculate_era_content(
        current: &mut Digest,
        latest_interval: u64,
    ) -> (Digest, HashSet<EraType>) {
        let mut eras_to_update = HashSet::new();
        let mut to_modify = HashSet::new();
        for (curr_era, interval_list) in current.eras.clone() {
            if curr_era == EraType::Hot || curr_era == EraType::Warm {
                for interval in &interval_list.content {
                    let new_era = Digest::get_era(&current.config, latest_interval, *interval);
                    if new_era != curr_era {
                        to_modify.insert((*interval, curr_era.clone(), new_era.clone()));
                        eras_to_update.insert(curr_era.clone());
                        eras_to_update.insert(new_era);
                    }
                }
            }
        }
        for (interval, prev_era, new_era) in to_modify {
            // move the interval from its previous era to the new
            current
                .eras
                .entry(prev_era)
                .and_modify(|e| e.content.retain(|&x| x != interval));
            current
                .eras
                .entry(new_era)
                .and_modify(|e| {
                    e.content.insert(interval);
                })
                .or_insert(Interval {
                    checksum: 0,
                    content: [interval].iter().cloned().collect(),
                });
        }

        (current.clone(), eras_to_update)
    }

    // compute the subinterval for a given timestamp
    fn get_subinterval(delta: Duration, ts: Timestamp, subintervals: usize) -> u64 {
        let ts = u64::try_from(
            ts.get_time()
                .to_system_time()
                .duration_since(super::EPOCH_START)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let delta = u64::try_from(delta.as_millis()).unwrap();
        ts / (delta / u64::try_from(subintervals).unwrap())
    }

    // compute the interval for a given subinterval
    fn get_interval(subinterval: u64, subintervals: usize) -> u64 {
        let subintervals = u64::try_from(subintervals).unwrap();
        subinterval / subintervals
    }

    // compute era for a given interval
    fn get_era(config: &DigestConfig, latest_interval: u64, interval: u64) -> EraType {
        let hot_min = latest_interval - u64::try_from(config.hot).unwrap() + 1;
        let warm_min = latest_interval
            - u64::try_from(config.hot).unwrap()
            - u64::try_from(config.warm).unwrap()
            + 1;

        if interval >= hot_min {
            EraType::Hot
        } else if interval >= warm_min {
            EraType::Warm
        } else {
            EraType::Cold
        }
    }
}

//functions for digest compression
impl Digest {
    // Compress the digest to transport on the wire
    pub fn compress(&self) -> Digest {
        let mut compressed_intervals = HashMap::new();
        let mut compressed_subintervals = HashMap::new();
        if self.eras.contains_key(&EraType::Hot) {
            for int in &self.eras.get(&EraType::Hot).unwrap().content {
                compressed_intervals.insert(*int, self.intervals.get(int).unwrap().clone());
                for sub in &self.intervals.get(int).unwrap().content {
                    let subinterval = self.subintervals.get(sub).unwrap().clone();
                    let comp_sub = SubInterval {
                        checksum: subinterval.checksum,
                        content: BTreeSet::new(),
                    };
                    compressed_subintervals.insert(*sub, comp_sub);
                }
            }
        };
        if self.eras.contains_key(&EraType::Warm) {
            for int in &self.eras.get(&EraType::Warm).unwrap().content {
                let interval = self.intervals.get(int).unwrap().clone();
                let comp_int = Interval {
                    checksum: interval.checksum,
                    content: BTreeSet::new(),
                };
                compressed_intervals.insert(*int, comp_int);
            }
        };
        let mut compressed_eras = HashMap::new();
        for era in self.eras.keys() {
            if era.clone() == EraType::Cold {
                compressed_eras.insert(
                    EraType::Cold,
                    Interval {
                        checksum: self.eras.get(era).unwrap().checksum,
                        content: BTreeSet::new(),
                    },
                );
            } else {
                compressed_eras.insert(era.clone(), self.eras.get(era).unwrap().clone());
            }
        }
        Digest {
            timestamp: self.timestamp,
            config: self.config.clone(),
            checksum: self.checksum,
            eras: compressed_eras,
            intervals: compressed_intervals,
            subintervals: compressed_subintervals,
        }
    }

    // get the intervals of a given era
    pub fn get_era_content(&self, era: &EraType) -> HashMap<u64, u64> {
        let mut result = HashMap::new();
        for int in self.eras.get(era).unwrap().content.clone() {
            result.insert(int, self.intervals.get(&int).unwrap().checksum);
        }
        result
    }

    // get the subintervals of a given interval
    pub fn get_interval_content(&self, intervals: HashSet<u64>) -> HashMap<u64, u64> {
        //return (subintervalid, checksum) for the set of intervals
        let mut result = HashMap::new();
        for each in intervals {
            for sub in self.intervals.get(&each).unwrap().content.clone() {
                result.insert(sub, self.subintervals.get(&sub).unwrap().checksum);
            }
        }
        result
    }

    // get the list of timestamps of a given subinterval
    pub fn get_subinterval_content(
        &self,
        subintervals: HashSet<u64>,
    ) -> HashMap<u64, BTreeSet<LogEntry>> {
        let mut result = HashMap::new();
        for each in subintervals {
            result.insert(each, self.subintervals.get(&each).unwrap().content.clone());
        }
        result
    }
}

// functions for alignment
impl Digest {
    // check if the other era has more content
    pub fn era_has_diff(&self, era: &EraType, other: &HashMap<EraType, Interval>) -> bool {
        match (other.get(era), self.eras.get(era)) {
            (Some(other_era), Some(my_era)) => other_era.checksum != my_era.checksum,
            (Some(_), None) => true,
            _ => false,
        }
    }

    // return mismatching intervals in an era
    pub fn get_interval_diff(&self, other_intervals: HashMap<u64, u64>) -> HashSet<u64> {
        let mut mis_int = HashSet::new();
        for int in other_intervals.keys() {
            if self.intervals.contains_key(int) {
                if *other_intervals.get(int).unwrap() != self.intervals.get(int).unwrap().checksum {
                    mis_int.insert(*int);
                }
            } else {
                mis_int.insert(*int);
            }
        }
        mis_int
    }

    // return mismatching subintervals in an interval
    pub fn get_subinterval_diff(&self, other_subintervals: HashMap<u64, u64>) -> HashSet<u64> {
        let mut mis_sub = HashSet::new();
        for sub in other_subintervals.keys() {
            if self.subintervals.contains_key(sub) {
                if *other_subintervals.get(sub).unwrap()
                    != self.subintervals.get(sub).unwrap().checksum
                {
                    mis_sub.insert(*sub);
                }
            } else {
                mis_sub.insert(*sub);
            }
        }
        mis_sub
    }

    // get missing content from a given list of subintervals
    pub fn get_full_content_diff(
        &self,
        other_subintervals: HashMap<u64, Vec<LogEntry>>,
    ) -> Vec<LogEntry> {
        let mut result = Vec::new();
        for (sub, content) in other_subintervals {
            let mut other = self.get_content_diff(sub, content.clone());
            result.append(&mut other);
        }
        result
    }

    //return missing content in a subinterval
    pub fn get_content_diff(&self, subinterval: u64, content: Vec<LogEntry>) -> Vec<LogEntry> {
        if !self.subintervals.contains_key(&subinterval) {
            return content;
        }
        let mut mis_content = Vec::new();
        for c in content {
            if !self
                .subintervals
                .get(&subinterval)
                .unwrap()
                .content
                .contains(&c)
            {
                mis_content.push(c);
            }
        }
        mis_content
    }
}

#[test]
fn test_create_digest_empty_initial() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = Digest::create_digest(
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        Vec::new(),
        1671612730,
    );
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 0,
        eras: HashMap::new(),
        intervals: HashMap::new(),
        subintervals: HashMap::new(),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_create_digest_with_initial_hot() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = Digest::create_digest(
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        vec![LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
        }],
        1671634800,
    );
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 6001159706341373391,
        eras: HashMap::from([(
            EraType::Hot,
            Interval {
                checksum: 4598971083408074426,
                content: BTreeSet::from([1671634800]),
            },
        )]),
        intervals: HashMap::from([(
            1671634800,
            Interval {
                checksum: 8436018757196527319,
                content: BTreeSet::from([16716348000]),
            },
        )]),
        subintervals: HashMap::from([(
            16716348000,
            SubInterval {
                checksum: 10827088509365589085,
                content: BTreeSet::from([LogEntry {
                    timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
                    key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
                }]),
            },
        )]),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_create_digest_with_initial_warm() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = Digest::create_digest(
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        vec![LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
        }],
        1671634810,
    );
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 6001159706341373391,
        eras: HashMap::from([(
            EraType::Warm,
            Interval {
                checksum: 4598971083408074426,
                content: BTreeSet::from([1671634800]),
            },
        )]),
        intervals: HashMap::from([(
            1671634800,
            Interval {
                checksum: 8436018757196527319,
                content: BTreeSet::from([16716348000]),
            },
        )]),
        subintervals: HashMap::from([(
            16716348000,
            SubInterval {
                checksum: 10827088509365589085,
                content: BTreeSet::from([LogEntry {
                    timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
                    key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
                }]),
            },
        )]),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_create_digest_with_initial_cold() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = Digest::create_digest(
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        vec![LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
        }],
        1671634910,
    );
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 6001159706341373391,
        eras: HashMap::from([(
            EraType::Cold,
            Interval {
                checksum: 4598971083408074426,
                content: BTreeSet::from([1671634800]),
            },
        )]),
        intervals: HashMap::from([(
            1671634800,
            Interval {
                checksum: 8436018757196527319,
                content: BTreeSet::from([16716348000]),
            },
        )]),
        subintervals: HashMap::from([(
            16716348000,
            SubInterval {
                checksum: 10827088509365589085,
                content: BTreeSet::from([LogEntry {
                    timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
                    key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
                }]),
            },
        )]),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_update_digest_add_content() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = async_std::task::block_on(Digest::update_digest(
        Digest {
            timestamp: Timestamp::from_str("2022-12-21T13:00:00.000000000Z/1").unwrap(),
            config: DigestConfig {
                delta: Duration::from_millis(1000),
                sub_intervals: 10,
                hot: 6,
                warm: 30,
            },
            checksum: 0,
            eras: HashMap::new(),
            intervals: HashMap::new(),
            subintervals: HashMap::new(),
        },
        1671634910,
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        HashSet::from([LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
        }]),
        HashSet::new(),
    ));
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 6001159706341373391,
        eras: HashMap::from([(
            EraType::Cold,
            Interval {
                checksum: 4598971083408074426,
                content: BTreeSet::from([1671634800]),
            },
        )]),
        intervals: HashMap::from([(
            1671634800,
            Interval {
                checksum: 8436018757196527319,
                content: BTreeSet::from([16716348000]),
            },
        )]),
        subintervals: HashMap::from([(
            16716348000,
            SubInterval {
                checksum: 10827088509365589085,
                content: BTreeSet::from([LogEntry {
                    timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
                    key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
                }]),
            },
        )]),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_update_digest_remove_content() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = async_std::task::block_on(Digest::update_digest(
        Digest {
            timestamp: Timestamp::from_str("2022-12-21T13:00:00.000000000Z/1").unwrap(),
            config: DigestConfig {
                delta: Duration::from_millis(1000),
                sub_intervals: 10,
                hot: 6,
                warm: 30,
            },
            checksum: 3304302629246049840,
            eras: HashMap::from([(
                EraType::Cold,
                Interval {
                    checksum: 8238986480495191270,
                    content: BTreeSet::from([1671634800]),
                },
            )]),
            intervals: HashMap::from([(
                1671634800,
                Interval {
                    checksum: 12344398372324783476,
                    content: BTreeSet::from([16716348000]),
                },
            )]),
            subintervals: HashMap::from([(
                16716348000,
                SubInterval {
                    checksum: 10007212639402189432,
                    content: BTreeSet::from([LogEntry {
                        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
                        key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
                    }]),
                },
            )]),
        },
        1671634910,
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        HashSet::new(),
        HashSet::from([LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("demo/example/a").unwrap(),
        }]),
    ));
    let expected = Digest {
        timestamp: Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        config: DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        checksum: 0,
        eras: HashMap::new(),
        intervals: HashMap::new(),
        subintervals: HashMap::new(),
    };
    assert_eq!(created, expected);
}

#[test]
fn test_update_remove_digest() {
    async_std::task::block_on(async {
        zenoh_core::zasync_executor_init!();
    });
    let created = Digest::create_digest(
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        DigestConfig {
            delta: Duration::from_millis(1000),
            sub_intervals: 10,
            hot: 6,
            warm: 30,
        },
        Vec::new(),
        1671612730,
    );
    let added = async_std::task::block_on(Digest::update_digest(
        created.clone(),
        1671612730,
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        HashSet::from([LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T12:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("a/b/c").unwrap(),
        }]),
        HashSet::new(),
    ));
    assert_ne!(created, added);

    let removed = async_std::task::block_on(Digest::update_digest(
        added.clone(),
        1671612730,
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        HashSet::new(),
        HashSet::from([LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T12:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("a/b/c").unwrap(),
        }]),
    ));
    assert_eq!(created, removed);

    let added_again = async_std::task::block_on(Digest::update_digest(
        removed,
        1671612730,
        Timestamp::from_str("2022-12-21T15:00:00.000000000Z/1").unwrap(),
        HashSet::from([LogEntry {
            timestamp: Timestamp::from_str("2022-12-21T12:00:00.000000000Z/1").unwrap(),
            key: OwnedKeyExpr::from_str("a/b/c").unwrap(),
        }]),
        HashSet::new(),
    ));
    assert_eq!(added, added_again);
}
