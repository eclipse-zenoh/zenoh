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

use flume::{Sender, TryRecvError};
use static_init::dynamic;
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

#[dynamic(lazy, drop)]
static mut GLOBAL_HANDOFF_REACTOR: HandoffReactor = HandoffReactor::new();

struct HandoffReactor {
    task_sender: Sender<Arc<TxHandoffTask>>,
}

impl HandoffReactor {
    fn new() -> Self {
        let (task_sender, task_receiver) = flume::unbounded();

        ZRuntime::Net.spawn(async move {
            // we mostly iterate over this rather than add\remove elements, so Vec should be optimal choice
            let mut tasks: Vec<Arc<TxHandoffTask>> = Vec::default();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                loop {
                    match task_receiver.try_recv() {
                        Ok(new_task) => tasks.push(new_task),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                tasks.retain(|t| {
                    t.poll();
                    Arc::strong_count(t) > 1
                });
            }
        });

        Self { task_sender }
    }
}

#[derive(Debug)]
struct TxHandoffTask {
    counter: ShmTXCounterLease,
    handoffs_len: AtomicUsize,
    handoffs: lockfree::queue::Queue<Option<ShmBufHardRef>>,
}

impl TxHandoffTask {
    fn new(counter: ShmTXCounterLease) -> Self {
        Self {
            counter,
            handoffs: Default::default(),
            handoffs_len: AtomicUsize::new(0),
        }
    }

    fn poll(&self) {
        let to_pop = self.handoffs_len.load(SeqCst) as isize - self.counter.counter() as isize;
        if to_pop > 0 {
            for _ in 0..to_pop {
                self.handoffs.pop();
            }
            self.handoffs_len.fetch_sub(to_pop as usize, SeqCst);
        }
    }
}

#[derive(Debug)]
pub struct TxHandoffInner {
    task: Arc<TxHandoffTask>,
    not_commit: Mutex<VecDeque<ShmBufHardRef>>,
}

impl TxHandoffInner {
    pub fn new(counter: ShmTXCounterLease) -> Self {
        let task = Arc::new(TxHandoffTask::new(counter));

        GLOBAL_HANDOFF_REACTOR
            .read()
            .task_sender
            .send(task.clone())
            .unwrap();

        Self {
            task,
            not_commit: Default::default(),
        }
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

    fn lock(&self) -> LockedTxHandoff {
        let inner = self.inner.clone();
        let lock = zlock!(self.inner.not_commit);

        // SAFETY: this is safe because we store Arc to inner together with this reference. Should
        // track that reference never leaves  LockedTxHandoff
        let lock: std::sync::MutexGuard<'static, VecDeque<ShmBufHardRef>> =
            unsafe { std::mem::transmute(lock) };
        LockedTxHandoff { inner, lock }
    }
}

struct LockedTxHandoff {
    inner: Arc<TxHandoffInner>,
    lock: std::sync::MutexGuard<'static, VecDeque<ShmBufHardRef>>,
}

impl LockedTxHandoff {
    pub fn push(&mut self, reference: ShmBufHardRef) {
        self.inner.task.counter.add(1);
        self.lock.push_back(reference);
    }

    pub fn cancel(mut self) {
        self.inner.task.counter.sub(self.lock.len() as u32);
        self.lock.clear();
    }

    pub fn commit(mut self) {
        let len = self.lock.len();
        while let Some(h) = self.lock.pop_front() {
            self.inner.task.handoffs.push(Some(h));
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
                let prio_container = prio_container.map(TxHandoff::new);
                Self(HandoffConfig::PerPrio(prio_container))
            }
            HandoffConfig::Disabled => Self(HandoffConfig::Disabled),
        }
    }
}

struct LazyTransaction<'a> {
    storage: &'a TxHandoffStorage,
    priority: Priority,
}

impl<'a> LazyTransaction<'a> {
    fn new(storage: &'a TxHandoffStorage, priority: Priority) -> Self {
        Self { storage, priority }
    }
}

enum OpenTransactionState<'a> {
    Lazy(LazyTransaction<'a>),
    Disabled,
    Init(LockedTxHandoff),
}

impl<'a> OpenTransactionState<'a> {
    fn try_access(&mut self) -> Option<&mut LockedTxHandoff> {
        match self {
            OpenTransactionState::Lazy(lazy_transaction) => {
                *self = match lazy_transaction.storage.get(lazy_transaction.priority) {
                    Some(handoff) => Self::Init(handoff.lock()),
                    None => Self::Disabled,
                };
                self.try_access()
            }
            OpenTransactionState::Disabled => None,
            OpenTransactionState::Init(locked_tx_handoff) => Some(locked_tx_handoff),
        }
    }

    fn on_tx(&mut self, reference: ShmBufHardRef) {
        if let Some(handoff) = self.try_access() {
            handoff.push(reference);
        }
    }

    fn close(self) -> ClosedTransactionState {
        if let OpenTransactionState::Init(locked) = self {
            return ClosedTransactionState::new(locked);
        }
        ClosedTransactionState::new_disabled()
    }
}

struct ClosedTransactionState {
    state: Option<LockedTxHandoff>,
}

impl ClosedTransactionState {
    fn new(state: LockedTxHandoff) -> Self {
        Self { state: Some(state) }
    }

    const fn new_disabled() -> Self {
        Self { state: None }
    }

    fn commit(self) {
        if let Some(state) = self.state {
            state.commit();
        }
    }

    fn cancel(self) {
        if let Some(state) = self.state {
            state.cancel();
        }
    }
}

pub struct OpenTransaction<'a> {
    state: OpenTransactionState<'a>,
}

impl<'a> OpenTransaction<'a> {
    pub fn new(storage: &'a TxHandoffStorage, priority: Priority) -> Self {
        let lazy = LazyTransaction::new(storage, priority);
        Self {
            state: OpenTransactionState::Lazy(lazy),
        }
    }

    pub const fn new_disabled() -> Self {
        Self {
            state: OpenTransactionState::Disabled,
        }
    }

    pub fn on_tx(&mut self, reference: ShmBufHardRef) {
        self.state.on_tx(reference);
    }

    pub fn close(self) -> TxHandoffTransaction {
        let closed = self.state.close();
        TxHandoffTransaction::new(closed)
    }
}

pub struct TxHandoffTransaction {
    state: ClosedTransactionState,
}

impl TxHandoffTransaction {
    fn new(state: ClosedTransactionState) -> Self {
        Self { state }
    }

    pub fn commit(&mut self) {
        let mut state = ClosedTransactionState::new_disabled();
        std::mem::swap(&mut self.state, &mut state);
        state.commit();
    }
}

impl Drop for TxHandoffTransaction {
    fn drop(&mut self) {
        let mut state = ClosedTransactionState::new_disabled();
        std::mem::swap(&mut self.state, &mut state);
        state.cancel();
    }
}

impl<W> WCodec<&PriorityContainer<ShmCounterID>, &mut W> for Zenoh080
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

impl<R> RCodec<PriorityContainer<ShmCounterID>, &mut R> for Zenoh080
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

#[cfg(test)]
mod race_tests {
    use std::{sync::atomic::Ordering::SeqCst, thread, time::Duration};

    use zenoh_core::Wait;
    use zenoh_shm::{
        api::{
            protocol_implementations::posix::posix_shm_provider_backend_binary_heap::PosixShmProviderBackendBinaryHeap,
            provider::shm_provider::ShmProviderBuilder,
        },
        ShmBufHardRef,
    };

    use super::*;
    use crate::unicast::establishment::ext::shm::segment::TXAuthSegment;

    #[test]
    fn concurrent_same_priority_transactions_are_isolated() {
        let backend = PosixShmProviderBackendBinaryHeap::builder(4096)
            .wait()
            .unwrap();
        let shm_provider = ShmProviderBuilder::backend(backend).wait();

        let buf_a = shm_provider.alloc(64).wait().unwrap();
        let buf_b = shm_provider.alloc(64).wait().unwrap();
        let ref_a = ShmBufHardRef::from(&buf_a);
        let ref_b = ShmBufHardRef::from(&buf_b);

        let auth_segment = Arc::new(TXAuthSegment::create(0, &[]).unwrap());
        let lease = ShmTXCounterLease::new(auth_segment).unwrap();
        let handoff = TxHandoff::new(lease);

        let h_a = handoff.clone();
        let h_b = handoff.clone();

        let a = thread::spawn(move || {
            let mut locked = h_a.lock();
            locked.push(ref_a);
            std::thread::sleep(Duration::from_millis(1000));
            // Simulate A's own `pipeline.push_network_message` failing: the transaction is
            // dropped without ever calling `commit()`, so only `cancel()` runs.
            locked.cancel();
        });

        let b = thread::spawn(move || {
            let mut locked = h_b.lock();
            locked.push(ref_b);
            std::thread::sleep(Duration::from_millis(500));
            locked.commit();
        });

        a.join().unwrap();
        b.join().unwrap();

        // What *should* hold if same-priority transactions were isolated: only B's entry was
        // ever committed, and A's cancel rolled back exactly A's own increment.
        //
        // What actually happens: B's `commit()` drains the shared `not_commit` queue wholesale,
        // taking A's still-staged entry with it. A's later `cancel()` finds nothing to roll
        // back, so A's counter increment leaks and A's hard ref is stuck in the handoff queue
        // permanently (no RX ack for a message that was never sent).
        let handoffs_len = handoff.inner.task.handoffs_len.load(SeqCst);
        let counter = handoff.inner.task.counter.counter();

        assert_eq!(
            handoffs_len, 1,
            "expected only B's transaction to be committed (isolated staging); \
             got {handoffs_len} committed entries — A's uncommitted push leaked into B's commit, \
             confirming `internal_schedule` does NOT serialize concurrent same-priority sends"
        );
        assert_eq!(
            counter, 1,
            "expected the counter to reflect only B's committed hard ref after A's cancel \
             rolled back its own increment; got {counter} — A's increment leaked"
        );
    }
}
