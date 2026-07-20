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
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc, Mutex,
    },
};

use tokio_util::sync::CancellationToken;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::zlock;
use zenoh_protocol::core::{Priority, Reliability};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;
use zenoh_shm::ShmBufHardRef;

use crate::{
    common::shm::handoff::PriorityContainer,
    unicast::establishment::ext::shm::{
        auth::AuthUnicast,
        segment::{RXAuthSegment, ShmCounterID, ShmRXCounterLease, ShmTXCounterLease},
    },
};

#[derive(Debug)]
pub enum HandoffConfig<T: Sized> {
    Disabled,
    PerPrio(PriorityContainer<T>),
}

impl<T: Sized + Clone> Clone for HandoffConfig<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::PerPrio(arg0) => Self::PerPrio(arg0.clone()),
        }
    }
}

impl<T: Sized> HandoffConfig<T> {
    pub fn from_fn(
        reliability: Reliability,
        ctor_fn: impl Fn(Priority) -> ZResult<T>,
    ) -> ZResult<Self> {
        Ok(match reliability {
            Reliability::BestEffort => Self::Disabled,
            Reliability::Reliable => Self::PerPrio(PriorityContainer::from_fn(ctor_fn)?),
        })
    }
}

pub type RxHandoffChannel = HandoffConfig<ShmRXCounterLease>;

impl RxHandoffChannel {
    pub fn new_rx(segment: &Arc<RXAuthSegment>, ids: HandoffCounterIds) -> Self {
        match ids {
            HandoffCounterIds::Disabled => Self::Disabled,
            HandoffCounterIds::PerPrio(prio_container) => {
                let prio_container = prio_container
                    .map(|counter_id| ShmRXCounterLease::new(segment.clone(), counter_id));
                Self::PerPrio(prio_container)
            }
        }
    }

    pub fn on_rx(&self, priority: Priority) {
        match self {
            HandoffConfig::PerPrio(prio_container) => prio_container[priority].counter_decrease(),
            HandoffConfig::Disabled => {}
        }
    }
}

#[derive(Debug)]
pub struct TxHandoffChannel(HandoffConfig<ShmTXCounterLease>);
impl TxHandoffChannel {
    pub fn new_disabled() -> Self {
        Self(HandoffConfig::Disabled)
    }

    pub fn new_tx(reliability: Reliability, auth: &AuthUnicast) -> ZResult<Self> {
        let ctor_fn = |_prio| auth.lease_tx_counter();
        Ok(Self(HandoffConfig::from_fn(reliability, ctor_fn)?))
    }

    pub fn ids(&self) -> HandoffCounterIds {
        match &self.0 {
            HandoffConfig::PerPrio(prio_container) => {
                let prio_container = prio_container.map_ref(|counter| counter.id());
                HandoffCounterIds::PerPrio(prio_container)
            }
            HandoffConfig::Disabled => HandoffCounterIds::Disabled,
        }
    }
}

#[derive(Debug)]
struct TxHandoffTask {
    counter: ShmTXCounterLease,
    handoffs_len: AtomicUsize,
    handoffs: lockfree::queue::Queue<ShmBufHardRef>,
}

impl TxHandoffTask {
    fn new(counter: ShmTXCounterLease) -> Self {
        Self {
            counter,
            handoffs: Default::default(),
            handoffs_len: AtomicUsize::new(0),
        }
    }
}

#[derive(Debug)]
pub struct TxHandoffInner {
    task: Arc<TxHandoffTask>,
    token: CancellationToken,
    not_commit: Mutex<VecDeque<ShmBufHardRef>>,
}

impl TxHandoffInner {
    pub fn new(counter: ShmTXCounterLease) -> Self {
        let task = Arc::new(TxHandoffTask::new(counter));
        let token = CancellationToken::new();

        let c_task = task.clone();
        let c_token = token.clone();
        ZRuntime::Net.spawn(async move {
            c_token
                .run_until_cancelled(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                        let to_pop = c_task.handoffs_len.load(SeqCst) as isize
                            - c_task.counter.counter() as isize;
                        if to_pop > 0 {
                            for _ in 0..to_pop {
                                c_task.handoffs.pop();
                            }
                            c_task.handoffs_len.fetch_sub(to_pop as usize, SeqCst);
                        }
                    }
                })
                .await
        });

        Self {
            task,
            token,
            not_commit: Default::default(),
        }
    }
}

impl Drop for TxHandoffInner {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

#[derive(Clone, Debug)]
pub struct TxHandoff {
    inner: Arc<TxHandoffInner>,
}

impl TxHandoff {
    pub fn new(counter: ShmTXCounterLease) -> Self {
        let inner = Arc::new(TxHandoffInner::new(counter));
        Self { inner }
    }

    pub fn push(&self, reference: ShmBufHardRef) {
        self.inner.task.counter.add(1);
        zlock!(self.inner.not_commit).push_back(reference);
    }

    pub fn cancel(&self) {
        let mut lock = zlock!(self.inner.not_commit);
        self.inner.task.counter.sub(lock.len() as u32);
        lock.clear();
    }

    pub fn commit(&self) {
        // NOTE: the sequence of operations below is important
        let mut lock = zlock!(self.inner.not_commit);

        let len = lock.len();
        while let Some(h) = lock.pop_front() {
            self.inner.task.handoffs.push(h);
        }
        self.inner.task.handoffs_len.fetch_add(len, SeqCst);
    }
}

#[derive(Debug, Clone)]
pub struct TxHandoffStorage(HandoffConfig<TxHandoff>);
impl TxHandoffStorage {
    pub fn new_disabled() -> Self {
        Self(HandoffConfig::Disabled)
    }

    fn get(&self, priority: Priority) -> Option<&TxHandoff> {
        match &self.0 {
            HandoffConfig::PerPrio(prio_container) => Some(&prio_container[priority]),
            HandoffConfig::Disabled => None,
        }
    }
}

impl From<TxHandoffChannel> for TxHandoffStorage {
    fn from(value: TxHandoffChannel) -> Self {
        match value.0 {
            HandoffConfig::PerPrio(prio_container) => {
                let prio_container = prio_container.map(|counter| TxHandoff::new(counter));
                Self(HandoffConfig::PerPrio(prio_container))
            }
            HandoffConfig::Disabled => Self(HandoffConfig::Disabled),
        }
    }
}

pub struct TxHandoffTransaction {
    handoff: TxHandoff,
}

impl TxHandoffTransaction {
    pub fn new(storage: &TxHandoffStorage, priority: Priority) -> Option<Self> {
        storage.get(priority).map(|handoff| Self {
            handoff: handoff.clone(),
        })
    }
}

impl TxHandoffTransaction {
    pub fn on_tx(&self, reference: ShmBufHardRef) {
        self.handoff.push(reference);
    }

    pub fn commit(&self) {
        self.handoff.commit();
    }
}

impl Drop for TxHandoffTransaction {
    fn drop(&mut self) {
        self.handoff.cancel();
    }
}

impl<'a, 'b, W> WCodec<&'a PriorityContainer<ShmCounterID>, &'b mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PriorityContainer<ShmCounterID>) -> Self::Output {
        for obj in x {
            self.write(&mut *writer, obj)?;
        }
        Ok(())
    }
}

impl<'a, R> RCodec<PriorityContainer<ShmCounterID>, &'a mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<PriorityContainer<ShmCounterID>, Self::Error> {
        PriorityContainer::from_fn(|_| self.read(&mut *reader))
    }
}

pub type HandoffCounterIds = HandoffConfig<ShmCounterID>;

impl<W> WCodec<&HandoffCounterIds, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &HandoffCounterIds) -> Self::Output {
        match x {
            HandoffConfig::Disabled => {
                self.write(&mut *writer, &0u8)?;
            }
            HandoffConfig::PerPrio(prio_container) => {
                self.write(&mut *writer, &1u8)?;
                self.write(&mut *writer, prio_container)?;
            }
        }
        Ok(())
    }
}

impl<R> RCodec<HandoffCounterIds, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<HandoffCounterIds, Self::Error> {
        let status: u8 = self.read(&mut *reader)?;
        Ok(if status == 0 {
            HandoffConfig::Disabled
        } else {
            let prio: PriorityContainer<ShmCounterID> = self.read(&mut *reader)?;
            HandoffCounterIds::PerPrio(prio)
        })
    }
}
