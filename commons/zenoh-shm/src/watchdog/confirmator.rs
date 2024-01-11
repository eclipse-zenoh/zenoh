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

use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc, RwLock},
    time::Duration,
};

use lazy_static::lazy_static;
use log::warn;
use thread_priority::*;
use zenoh_result::{zerror, ZResult};

use super::{
    descriptor::{Descriptor, OwnedDescriptor, SegmentID},
    segment::Segment,
};

lazy_static! {
    pub static ref GLOBAL_CONFIRMATOR: WatchdogConfirmator =
        WatchdogConfirmator::new(Duration::from_millis(50));
}

pub struct ConfirmedDescriptor {
    pub owned: OwnedDescriptor,
    confirmed: Arc<ConfirmedSegment>,
}

impl Drop for ConfirmedDescriptor {
    fn drop(&mut self) {
        self.confirmed.remove(self.owned.clone());
    }
}

impl ConfirmedDescriptor {
    fn new(
        owned: OwnedDescriptor,
        confirmed: Arc<ConfirmedSegment>,
    ) -> Self {
        owned.confirm();
        confirmed.add(owned.clone());
        Self { owned, confirmed }
    }
}

#[derive(PartialEq)]
enum Transaction {
    Add,
    Remove,
}

struct ConfirmedSegment {
    segment: Arc<Segment>,
    transactions: lockfree::queue::Queue<(Transaction, OwnedDescriptor)>,
}

impl ConfirmedSegment {
    fn new(segment: Arc<Segment>) -> Self {
        Self {
            segment,
            transactions: lockfree::queue::Queue::default(),
        }
    }

    fn add(&self, descriptor: OwnedDescriptor) {
        self.transactions.push( (Transaction::Add, descriptor));
    }

    fn remove(&self, descriptor: OwnedDescriptor) {
        self.transactions.push( (Transaction::Remove, descriptor));
    }

    fn collect_transactions(&self, watchdogs: &mut BTreeMap<OwnedDescriptor, i32>) {
        while let Some((transaction, descriptor)) = self.transactions.pop() {
            // collect transactions
            match watchdogs.entry(descriptor) {
                std::collections::btree_map::Entry::Vacant(vacant) => {
                    #[cfg(feature = "test")]
                    assert!( transaction == Transaction::Add );
                    vacant.insert(1);
                },
                std::collections::btree_map::Entry::Occupied(mut occupied) => match transaction {
                    Transaction::Add => {
                        *occupied.get_mut() += 1;
                    },
                    Transaction::Remove => {
                        if *occupied.get() == 1 {
                            occupied.remove();
                        } else {
                            *occupied.get_mut() -= 1;
                        }
                    },
                },
            }
        }
    }
}
unsafe impl Send for ConfirmedSegment {}
unsafe impl Sync for ConfirmedSegment {}

// todo: optimize confirmation by packing descriptors AND linked table together
// todo: think about linked table cleanup
pub struct WatchdogConfirmator {
    confirmed: RwLock<BTreeMap<SegmentID, Arc<ConfirmedSegment>>>,
    segment_transactions: Arc<lockfree::queue::Queue<Arc<ConfirmedSegment>>>,
    running: Arc<AtomicBool>,
}

impl Drop for WatchdogConfirmator {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl WatchdogConfirmator {
    fn new(interval: Duration) -> Self {
        let segment_transactions = Arc::<lockfree::queue::Queue<Arc<ConfirmedSegment>>>::default();
        let running = Arc::new(AtomicBool::new(true));

        let c_segment_transactions = segment_transactions.clone();
        let c_running = running.clone();

        let _ = ThreadBuilder::default()
            .name("Watchdog Confirmator thread")
            //.policy(ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Deadline))
            //.priority(ThreadPriority::Deadline { runtime: Duration::from_micros(100), deadline: interval, period: interval, flags: DeadlineFlags::default() })
            //.policy(ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo))
            .priority(ThreadPriority::Crossplatform(ThreadPriorityValue::try_from(48).unwrap()))
            .spawn(move |result| {
                    if let Err(e) = result {
                        let ret = std::io::Error::last_os_error().to_string();
                        panic!("Watchdog Confirmator: error setting thread priority: {:?}, will continue operating with default priority...", e);
                        panic!("");
                    }

                    let mut segments: Vec<(Arc<ConfirmedSegment>, BTreeMap<OwnedDescriptor, i32>)> = vec![];
                    while c_running.load(std::sync::atomic::Ordering::Relaxed) {
                        let cycle_start = std::time::Instant::now();

                        // add new segments
                        while let Some(new_segment) = c_segment_transactions.as_ref().pop() {
                            segments.push((new_segment, BTreeMap::default()));
                        }

                        // collect all existing transactions
                        for (segment, watchdogs) in &mut segments {
                            segment.collect_transactions(watchdogs);
                        }

                        // confirm all tracked watchdogs
                        for (_, watchdogs) in &segments {
                            for watchdog in watchdogs {
                                watchdog.0.confirm();
                            }
                        }

                        // sleep for next iteration
                        let elapsed = cycle_start.elapsed();
                        if elapsed < interval {
                            let sleep_interval = interval - elapsed;
                            std::thread::sleep(sleep_interval);
                        } else {
                            warn!("Watchdog confirmation timer overrun!");
                            #[cfg(feature = "test")]
                            panic!("Watchdog confirmation timer overrun!");
                        }    
                    }
                });

        Self {
            confirmed: RwLock::default(),
            segment_transactions,
            running,
        }
    }

    pub fn add_owned(&self, descriptor: &OwnedDescriptor) -> ZResult<ConfirmedDescriptor> {
        self.add(&Descriptor::from(descriptor))
    }

    pub fn add(&self, descriptor: &Descriptor) -> ZResult<ConfirmedDescriptor> {
        let guard = self.confirmed.read().map_err(|e| zerror!("{e}"))?;
        if let Some(segment) = guard.get(&descriptor.id) {
            return self.link(descriptor, segment);
        }
        drop(guard);

        let segment = Arc::new(Segment::open(descriptor.id)?);
        let confirmed_segment = Arc::new(ConfirmedSegment::new(segment));
        let confirmed_descriptoir = self.link(descriptor, &confirmed_segment);

        let mut guard = self.confirmed.write().map_err(|e| zerror!("{e}"))?;
        match guard.entry(descriptor.id) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                vacant.insert(confirmed_segment.clone());
                self.segment_transactions.push(confirmed_segment);
                confirmed_descriptoir
            }
            std::collections::btree_map::Entry::Occupied(occupied) => {
                self.link(descriptor, occupied.get())
            }
        }
    }


    fn link(&self, descriptor: &Descriptor, segment: &Arc<ConfirmedSegment>) -> ZResult<ConfirmedDescriptor> {
        let index = descriptor.index_and_bitpos >> 6;
        let bitpos = descriptor.index_and_bitpos & 0x3f;

        let atomic = unsafe { segment.segment.array.elem(index) };
        let mask = 1u64 << bitpos;

        let owned = OwnedDescriptor::new(segment.segment.clone(), atomic, mask);
        let confirmed = ConfirmedDescriptor::new(owned, segment.clone());
        Ok(confirmed)
    }
}
