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
    collections::{btree_map::Entry, BTreeMap},
    ops::Deref,
    sync::{atomic::AtomicI32, Arc, RwLock},
    time::Duration,
};

use crossbeam_queue::SegQueue;
use static_init::dynamic;
use zenoh_core::{zread, zwrite};

use super::periodic_task::PeriodicTask;
use crate::metadata::{
    descriptor::{MetadataSegmentID, OwnedMetadataDescriptor, OwnedWatchdog},
    segment::MetadataSegment,
};

#[dynamic(lazy, drop)]
pub static mut GLOBAL_CONFIRMATOR: WatchdogConfirmator =
    WatchdogConfirmator::new(Duration::from_millis(50));

#[derive(Debug)]
pub struct ConfirmedDescriptor {
    pub owned: OwnedMetadataDescriptor,
    confirmed: Arc<ConfirmedSegment>,
}

impl Drop for ConfirmedDescriptor {
    fn drop(&mut self) {
        self.confirmed.remove(self.owned.clone());
    }
}

impl ConfirmedDescriptor {
    fn new(owned: OwnedMetadataDescriptor, confirmed: Arc<ConfirmedSegment>) -> Self {
        confirmed.add(owned.clone());
        Self { owned, confirmed }
    }
}

#[derive(PartialEq)]
enum Transaction {
    Add(OwnedWatchdog),
    Remove(OwnedWatchdog),
}

#[derive(Debug)]
struct ConfirmedSegment {
    _segment: Arc<MetadataSegment>,

    cap: AtomicI32,
    sender: crossbeam_channel::Sender<Transaction>,
    receiver: crossbeam_channel::Receiver<Transaction>,
    task_kick: std::sync::mpsc::Sender<()>,
}

impl ConfirmedSegment {
    fn new(segment: Arc<MetadataSegment>, task_kick: std::sync::mpsc::Sender<()>) -> Self {
        let channel_size = 65536 * 2;
        let (sender, receiver) = crossbeam_channel::bounded::<Transaction>(channel_size);
        Self {
            _segment: segment,
            cap: AtomicI32::new((channel_size / 2) as i32),
            sender,
            receiver,
            task_kick,
        }
    }

    fn add(&self, descriptor: OwnedMetadataDescriptor) {
        if self.cap.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 0 {
            let _ = self.task_kick.send(());
        }
        self.sender
            .try_send(Transaction::Add(descriptor.deref().clone()))
            .unwrap();
    }

    fn remove(&self, descriptor: OwnedMetadataDescriptor) {
        if self.cap.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 0 {
            let _ = self.task_kick.send(());
        }

        self.sender
            .try_send(Transaction::Remove(descriptor.deref().clone()))
            .unwrap();
    }

    // See ordering implementation for OwnedMetadataDescriptor
    #[allow(clippy::mutable_key_type)]
    fn collect_transactions(&self, watchdogs: &mut BTreeMap<OwnedWatchdog, i32>) {
        while let Ok(transaction) = self.receiver.try_recv() {
            self.cap.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            match transaction {
                Transaction::Add(watchdog) => match watchdogs.entry(watchdog) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(1);
                    }
                    Entry::Occupied(mut occupied) => {
                        *occupied.get_mut() += 1;
                    }
                },
                Transaction::Remove(watchdog) => match watchdogs.entry(watchdog) {
                    Entry::Vacant(_) => {
                        #[cfg(feature = "test")]
                        panic!("Removing not existing entry");
                    }
                    Entry::Occupied(mut occupied) => {
                        if *occupied.get() == 1 {
                            occupied.remove();
                        } else {
                            *occupied.get_mut() -= 1;
                        }
                    }
                },
            }
        }
    }
}

// TODO: optimize confirmation by packing descriptors AND linked table together
// TODO: think about linked table cleanup
pub struct WatchdogConfirmator {
    confirmed: RwLock<BTreeMap<MetadataSegmentID, Arc<ConfirmedSegment>>>,
    segment_transactions: Arc<SegQueue<Arc<ConfirmedSegment>>>,
    _task: PeriodicTask,
}

impl WatchdogConfirmator {
    fn new(interval: Duration) -> Self {
        let segment_transactions = Arc::<SegQueue<Arc<ConfirmedSegment>>>::default();

        let c_segment_transactions = segment_transactions.clone();
        let mut segments: Vec<(Arc<ConfirmedSegment>, BTreeMap<OwnedWatchdog, i32>)> = vec![];
        let task = PeriodicTask::new("Watchdog Confirmator".to_owned(), interval, move || {
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
        });

        Self {
            confirmed: RwLock::default(),
            segment_transactions,
            _task: task,
        }
    }

    pub fn add(&self, descriptor: OwnedMetadataDescriptor) -> ConfirmedDescriptor {
        // confirm ASAP!
        descriptor.confirm();

        let guard = zread!(self.confirmed);
        if let Some(segment) = guard.get(&descriptor.segment.data.id()) {
            return ConfirmedDescriptor::new(descriptor, segment.clone());
        }
        drop(guard);

        let confirmed_segment = Arc::new(ConfirmedSegment::new(
            descriptor.segment.clone(),
            self._task.kicker(),
        ));
        let confirmed_descriptoir =
            ConfirmedDescriptor::new(descriptor.clone(), confirmed_segment.clone());

        let mut guard = zwrite!(self.confirmed);
        match guard.entry(descriptor.segment.data.id()) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                vacant.insert(confirmed_segment.clone());
                self.segment_transactions.push(confirmed_segment);
                confirmed_descriptoir
            }
            std::collections::btree_map::Entry::Occupied(occupied) => {
                // this is intentional
                ConfirmedDescriptor::new(descriptor, occupied.get().clone())
            }
        }
    }
}
