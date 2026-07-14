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

use std::{
    ops::{Index, IndexMut},
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use tokio_util::sync::CancellationToken;
use zenoh_protocol::core::Priority;
use zenoh_runtime::ZRuntime;
use zenoh_shm::{handoff::Handoff, ShmBufInner};

use crate::unicast::establishment::ext::shm::segment::ShmTXCounterLease;

struct TxHandoffInner {
    counter: ShmTXCounterLease,
    handoffs_len: AtomicUsize,
    handoffs: lockfree::queue::Queue<Handoff>,
}

impl TxHandoffInner {
    fn new(counter: ShmTXCounterLease) -> Self {
        Self {
            counter,
            handoffs: Default::default(),
            handoffs_len: AtomicUsize::new(0),
        }
    }
}

pub struct TxHandoff {
    inner: Arc<TxHandoffInner>,
}

impl TxHandoff {
    pub fn new(counter: ShmTXCounterLease, cancellation_token: CancellationToken) -> Self {
        let inner = Arc::new(TxHandoffInner::new(counter));

        let c_inner = inner.clone();
        ZRuntime::Net.spawn(async move {
            cancellation_token
                .run_until_cancelled(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                        let to_pop = c_inner.handoffs_len.load(SeqCst) as isize
                            - c_inner.counter.counter() as isize;
                        if to_pop > 0 {
                            for _ in 0..to_pop {
                                c_inner.handoffs.pop();
                            }
                            c_inner.handoffs_len.fetch_sub(to_pop as usize, SeqCst);
                        }
                    }
                })
                .await
        });

        Self { inner }
    }

    pub fn on_tx(&mut self, shm_buf: Arc<ShmBufInner>) {
        // NOTE: the sequence of operations below is important
        self.inner.counter.counter_increase();
        self.inner.handoffs.push(shm_buf.into());
        self.inner.handoffs_len.fetch_add(1, SeqCst);
    }
}

#[derive(Debug)]
pub struct PriorityContainer<T: Sized> {
    per_prio_objects: [T; Priority::NUM],
}

impl<T: Sized> PriorityContainer<T> {
    pub fn new(per_prio_objects: [T; Priority::NUM]) -> Self {
        Self { per_prio_objects }
    }
    
    pub fn from_fn<E>(mut ctor_fn: impl FnMut(Priority) -> Result<T, E>) -> Result<Self, E> {
        // TODO: std::array::try_from_fn is unstable yet...
        let per_prio_objects = [
            ctor_fn(Priority::Control)?,
            ctor_fn(Priority::RealTime)?,
            ctor_fn(Priority::InteractiveHigh)?,
            ctor_fn(Priority::InteractiveLow)?,
            ctor_fn(Priority::DataHigh)?,
            ctor_fn(Priority::Data)?,
            ctor_fn(Priority::DataLow)?,
            ctor_fn(Priority::Background)?,
        ];
        Ok(Self { per_prio_objects })
    }

    pub fn from_fn_infallable(ctor_fn: impl Fn(Priority) -> T) -> Self {
        let per_prio_objects = std::array::from_fn(|prio| {
            // SAFETY: `Priority` is guaranteed to be in the range 0..=7, so this conversion is safe.
            ctor_fn(unsafe { (prio as u8).try_into().unwrap_unchecked() })
        });
        Self { per_prio_objects }
    }

    pub fn map<Tother: Sized>(
        &self,
        map_fn: impl Fn(&T) -> Tother,
    ) -> PriorityContainer<Tother> {
        PriorityContainer::from_fn_infallable(|prio| {
            let obj = &self.per_prio_objects[prio as usize];
            map_fn(obj)
        })
    }
}

impl<'a, T: Sized> IntoIterator for &'a PriorityContainer<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        (&self.per_prio_objects).into_iter()
    }
}

impl<T: Sized> Index<Priority> for PriorityContainer<T> {
    type Output = T;

    fn index(&self, index: Priority) -> &Self::Output {
        &self.per_prio_objects[index as usize]
    }
}

impl<T: Sized> IndexMut<Priority> for PriorityContainer<T> {
    fn index_mut(&mut self, index: Priority) -> &mut Self::Output {
        &mut self.per_prio_objects[index as usize]
    }
}
