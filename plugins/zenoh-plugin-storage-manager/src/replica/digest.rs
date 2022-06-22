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

use crc::{Crc, CRC_64_ECMA_182};
use derive_new::new;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::str::FromStr;
use std::string::ParseError;
use std::time::Duration;
use zenoh::time::Timestamp;

#[derive(Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct DigestConfig {
    pub delta: Duration,
    pub sub_intervals: usize,
    pub hot: usize,
    pub warm: usize,
}

// TODO: split this such that only minimal information is sent through wire. Maybe a new CompressedDigest struct would also work
#[derive(Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct Digest {
    pub timestamp: zenoh::time::Timestamp,
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
    pub content: Vec<u64>,
}

#[derive(new, Eq, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct SubInterval {
    #[new(default)]
    pub checksum: u64,
    #[new(default)]
    pub content: Vec<zenoh::time::Timestamp>,
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
            _ => Ok(EraType::Cold), //TODO: fix this later
        }
    }
}

trait Checksum {
    fn format_content(&self) -> String;
}

impl Checksum for Timestamp {
    fn format_content(&self) -> String {
        format!("{}", self)
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
    pub fn create_digest(
        timestamp: zenoh::time::Timestamp,
        config: DigestConfig,
        raw_log: Vec<Timestamp>,
        latest_interval: u64,
    ) -> Digest {
        let subinterval_content = Digest::get_subintervals(raw_log, &config);
        let interval_content = Digest::get_intervals(
            subinterval_content.keys().cloned().collect(),
            config.sub_intervals,
        );
        let era_content = Digest::get_eras(
            interval_content.keys().cloned().collect(),
            &config,
            latest_interval,
        );

        let mut subinterval_hash = HashMap::new();
        for (sub, content) in subinterval_content {
            let subinterval = SubInterval {
                checksum: Digest::get_subinterval_checksum(&content),
                content,
            };
            subinterval_hash.insert(sub, subinterval);
        }

        let mut interval_hash = HashMap::new();
        for (int, content) in interval_content {
            let interval = Interval {
                checksum: Digest::get_interval_checksum(&content, &subinterval_hash),
                content,
            };
            interval_hash.insert(int, interval);
        }

        let mut era_hash = HashMap::new();
        for (e, content) in era_content {
            let era = Interval {
                checksum: Digest::get_era_checksum(&content, &interval_hash),
                content,
            };
            era_hash.insert(e, era);
        }

        Digest {
            timestamp,
            config,
            checksum: Digest::get_digest_checksum(&era_hash),
            eras: era_hash,
            intervals: interval_hash,
            subintervals: subinterval_hash,
        }
    }

    fn get_content_hash<T: Checksum>(content: &[T]) -> u64 {
        let crc64 = Crc::<u64>::new(&CRC_64_ECMA_182);
        let mut hasher = crc64.digest();
        for s_cont in content {
            let formatted = s_cont.format_content();
            hasher.update(formatted.as_bytes());
        }
        hasher.finalize()
    }

    fn get_subinterval_checksum(content: &[Timestamp]) -> u64 {
        Digest::get_content_hash(content)
    }

    fn get_interval_checksum(content: &[u64], info: &HashMap<u64, SubInterval>) -> u64 {
        let mut hashable_content = Vec::new();
        for i in content {
            let i_cont = info.get(i).unwrap().checksum;
            hashable_content.push(i_cont);
        }
        Digest::get_content_hash(&hashable_content)
    }

    fn get_era_checksum(content: &[u64], info: &HashMap<u64, Interval>) -> u64 {
        let mut hashable_content = Vec::new();
        for i in content {
            let i_cont = info.get(i).unwrap().checksum;
            hashable_content.push(i_cont);
        }
        Digest::get_content_hash(&hashable_content)
    }

    fn get_digest_checksum(content: &HashMap<EraType, Interval>) -> u64 {
        let mut hashable_content = Vec::new();
        for i_cont in content.values() {
            hashable_content.push(i_cont.checksum);
        }
        Digest::get_content_hash(&hashable_content)
    }

    fn get_subintervals(
        log: Vec<Timestamp>,
        config: &DigestConfig,
    ) -> HashMap<u64, Vec<Timestamp>> {
        let mut subinterval_content: HashMap<u64, Vec<Timestamp>> = HashMap::new();
        for ts in log {
            let sub = Digest::get_subinterval(config.delta, ts, config.sub_intervals);
            subinterval_content
                .entry(sub)
                .and_modify(|e| e.push(ts))
                .or_insert_with(|| vec![ts]);
        }
        for content in subinterval_content.values_mut() {
            content.sort_unstable();
            content.dedup();
        }
        subinterval_content
    }

    fn get_intervals(
        subinterval_list: HashSet<u64>,
        subintervals: usize,
    ) -> HashMap<u64, Vec<u64>> {
        let mut interval_content: HashMap<u64, Vec<u64>> = HashMap::new();
        for sub in subinterval_list {
            let interval = Digest::get_interval(sub, subintervals);
            interval_content
                .entry(interval)
                .and_modify(|e| e.push(sub))
                .or_insert_with(|| vec![sub]);
        }
        for content in interval_content.values_mut() {
            content.sort_unstable();
            content.dedup();
        }
        interval_content
    }

    fn get_eras(
        interval_list: HashSet<u64>,
        config: &DigestConfig,
        latest_interval: u64,
    ) -> HashMap<EraType, Vec<u64>> {
        let mut era_content: HashMap<EraType, Vec<u64>> = HashMap::new();
        for int in interval_list {
            let era = Digest::get_era(config, latest_interval, int);
            era_content
                .entry(era)
                .and_modify(|e| e.push(int))
                .or_insert_with(|| vec![int]);
        }
        for content in era_content.values_mut() {
            content.sort_unstable();
            content.dedup();
        }
        era_content
    }

    // fn populate_content(
    //     processed_log: Vec<Timestamp>,
    // ) -> (
    //     HashMap<u64, Vec<Timestamp>>,
    //     HashMap<u64, Vec<u64>>,
    //     HashMap<EraType, Vec<u64>>,
    // ) {
    //     let mut subinterval_content = HashMap::new();
    //     let mut interval_content = HashMap::new();
    //     let mut era_content = HashMap::new();
    //     for log_entry in processed_log {
    //         subinterval_content
    //             .entry(log_entry.subinterval)
    //             .or_insert_with(Vec::new);
    //         subinterval_content
    //             .get_mut(&log_entry.subinterval)
    //             .unwrap()
    //             .push(log_entry.content);
    //         interval_content
    //             .entry(log_entry.interval)
    //             .or_insert_with(Vec::new);
    //         interval_content
    //             .get_mut(&log_entry.interval)
    //             .unwrap()
    //             .push(log_entry.subinterval);
    //         if !era_content.contains_key(&log_entry.era) {
    //             era_content.insert(log_entry.era.clone(), Vec::new());
    //         }
    //         era_content
    //             .get_mut(&log_entry.era)
    //             .unwrap()
    //             .push(log_entry.interval);
    //     }

    //     for content in subinterval_content.values_mut() {
    //         content.sort_unstable();
    //         content.dedup();
    //     }
    //     for content in interval_content.values_mut() {
    //         content.sort_unstable();
    //         content.dedup();
    //     }
    //     for content in era_content.values_mut() {
    //         content.sort_unstable();
    //         content.dedup();
    //     }
    //     (subinterval_content, interval_content, era_content)
    // }

    pub async fn update_digest(
        mut current: Digest,
        latest_interval: u64,
        last_snapshot_time: Timestamp,
        content: HashSet<Timestamp>,
        redundant_content: HashSet<Timestamp>,
    ) -> Digest {
        // push content in correct places
        let (mut current, mut subintervals_to_update, mut intervals_to_update, mut eras_to_update) =
            Digest::update_content(&mut current, content, latest_interval);

        // remove redundant content from proper places
        let (mut current, further_subintervals, further_intervals, further_eras) =
            Digest::remove_redundant_content(&mut current, redundant_content, latest_interval);

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

        // reconstruct updates parts
        for sub in subintervals_to_update {
            // order the content, hash them
            subintervals.get_mut(&sub).unwrap().content.sort_unstable();
            subintervals.get_mut(&sub).unwrap().content.dedup();
            let checksum =
                Digest::get_subinterval_checksum(&subintervals.get(&sub).unwrap().content);

            subintervals.get_mut(&sub).unwrap().checksum = checksum;
        }

        for int in intervals_to_update {
            // order the content, hash them
            intervals.get_mut(&int).unwrap().content.sort_unstable();
            intervals.get_mut(&int).unwrap().content.dedup();
            let checksum =
                Digest::get_interval_checksum(&intervals.get(&int).unwrap().content, &subintervals);

            intervals.get_mut(&int).unwrap().checksum = checksum;
        }

        for era in eras_to_update {
            // order the content, hash them
            eras.get_mut(&era).unwrap().content.sort_unstable();
            eras.get_mut(&era).unwrap().content.dedup();
            let checksum = Digest::get_era_checksum(&eras.get(&era).unwrap().content, &intervals);

            eras.get_mut(&era).unwrap().checksum = checksum;
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

    fn update_content(
        current: &mut Digest,
        content: HashSet<Timestamp>,
        latest_interval: u64,
    ) -> (Digest, HashSet<u64>, HashSet<u64>, HashSet<EraType>) {
        let mut eras_to_update = HashSet::new();
        let mut intervals_to_update = HashSet::new();
        let mut subintervals_to_update = HashSet::new();

        for ts in content {
            let subinterval =
                Digest::get_subinterval(current.config.delta, ts, current.config.sub_intervals);
            subintervals_to_update.insert(subinterval);
            let interval = Digest::get_interval(subinterval, current.config.sub_intervals);
            intervals_to_update.insert(interval);
            let era = Digest::get_era(&current.config, latest_interval, interval);
            eras_to_update.insert(era.clone());

            current
                .subintervals
                .entry(subinterval)
                .and_modify(|e| e.content.push(ts))
                .or_insert(SubInterval {
                    checksum: 0,
                    content: vec![ts],
                });
            current
                .intervals
                .entry(interval)
                .and_modify(|e| e.content.push(subinterval))
                .or_insert(Interval {
                    checksum: 0,
                    content: vec![subinterval],
                });
            current
                .eras
                .entry(era)
                .and_modify(|e| e.content.push(interval))
                .or_insert(Interval {
                    checksum: 0,
                    content: vec![interval],
                });
        }

        (
            current.clone(),
            subintervals_to_update,
            intervals_to_update,
            eras_to_update,
        )
    }

    fn remove_redundant_content(
        current: &mut Digest,
        redundant_content: HashSet<Timestamp>,
        latest_interval: u64,
    ) -> (Digest, HashSet<u64>, HashSet<u64>, HashSet<EraType>) {
        let mut eras_to_update = HashSet::new();
        let mut intervals_to_update = HashSet::new();
        let mut subintervals_to_update = HashSet::new();

        for entry in redundant_content {
            let subinterval =
                Digest::get_subinterval(current.config.delta, entry, current.config.sub_intervals);
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
                    .dedup();
                current
                    .subintervals
                    .get_mut(&subinterval)
                    .unwrap()
                    .content
                    .retain(|&x| x.get_time() != entry.get_time() && x.get_id() != entry.get_id());
                subintervals_to_update.insert(subinterval);
            }
            if current.intervals.contains_key(&interval) {
                current
                    .intervals
                    .get_mut(&interval)
                    .unwrap()
                    .content
                    .dedup();
                current
                    .intervals
                    .get_mut(&interval)
                    .unwrap()
                    .content
                    .retain(|&x| x != subinterval);
                intervals_to_update.insert(interval);
            }
            if current.eras.contains_key(&era) {
                current.eras.get_mut(&era).unwrap().content.dedup();
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
                .and_modify(|e| e.content.dedup())
                .and_modify(|e| e.content.retain(|&x| x != interval));
            current
                .eras
                .entry(new_era)
                .and_modify(|e| e.content.push(interval))
                .or_insert(Interval {
                    checksum: 0,
                    content: vec![interval],
                });
        }

        (current.clone(), eras_to_update)
    }

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

    fn get_interval(subinterval: u64, subintervals: usize) -> u64 {
        let subintervals = u64::try_from(subintervals).unwrap();
        subinterval / subintervals
    }

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
                        content: Vec::new(),
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
                    content: Vec::new(),
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
                        content: Vec::new(),
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

    pub fn get_era_content(&self, era: EraType) -> HashMap<u64, u64> {
        let mut result = HashMap::new();
        for int in self.eras.get(&era).unwrap().content.clone() {
            result.insert(int, self.intervals.get(&int).unwrap().checksum);
        }
        result
    }

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

    pub fn get_subinterval_content(
        &self,
        subintervals: HashSet<u64>,
    ) -> HashMap<u64, Vec<zenoh::time::Timestamp>> {
        let mut result = HashMap::new();
        for each in subintervals {
            result.insert(each, self.subintervals.get(&each).unwrap().content.clone());
        }
        result
    }
}

//functions for alignment
impl Digest {
    //return mismatching eras
    pub fn get_era_diff(&self, other: HashMap<EraType, Interval>) -> HashSet<EraType> {
        let mut result = HashSet::new();
        for era in vec![EraType::Hot, EraType::Warm, EraType::Cold] {
            if other.contains_key(&era) && other.get(&era).unwrap().checksum != 0 {
                if self.eras.contains_key(&era) {
                    if self.eras.get(&era).unwrap().checksum != other.get(&era).unwrap().checksum {
                        result.insert(era);
                    }
                } else {
                    result.insert(era);
                }
            } // else no need to check
        }
        result
    }

    //return mismatching intervals in an era
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

    //return mismatching subintervals in an interval
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

    pub fn get_full_content_diff(
        &self,
        other_subintervals: HashMap<u64, Vec<zenoh::time::Timestamp>>,
    ) -> Vec<zenoh::time::Timestamp> {
        let mut result = Vec::new();
        for (sub, content) in other_subintervals {
            let mut other = self.get_content_diff(sub, content.clone());
            result.append(&mut other);
        }
        result
    }

    //return missing content in a subinterval
    pub fn get_content_diff(
        &self,
        subinterval: u64,
        content: Vec<zenoh::time::Timestamp>,
    ) -> Vec<zenoh::time::Timestamp> {
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
