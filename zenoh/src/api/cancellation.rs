//
// Copyright (c) 2025 ZettaScale Technology
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
use core::fmt;
use std::{
    collections::HashMap,
    future::IntoFuture,
    ops::DerefMut,
    sync::{Arc, Mutex, OnceLock},
};

use futures::{future::BoxFuture, FutureExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

#[zenoh_macros::pub_visibility_if_internal]
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct SyncGroupNotifier(OwnedSemaphorePermit);

#[zenoh_macros::pub_visibility_if_internal]
#[derive(Clone)]
pub(crate) struct SyncGroup {
    semaphore: Arc<Semaphore>,
}

impl Default for SyncGroup {
    fn default() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(SyncGroup::max_permits() as usize)),
        }
    }
}
impl SyncGroup {
    fn max_permits() -> u32 {
        Semaphore::MAX_PERMITS.try_into().unwrap_or(u32::MAX)
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) fn notifier(&self) -> Option<SyncGroupNotifier> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(SyncGroupNotifier)
    }

    pub(crate) fn close(&self) {
        self.semaphore.close();
    }

    pub(crate) fn num_active_notifiers(&self) -> usize {
        SyncGroup::max_permits() as usize - self.semaphore.available_permits()
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) fn wait(&self) {
        let s = self.semaphore.clone();
        let _p = futures::executor::block_on(s.acquire_many(Self::max_permits()));
        self.close();
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) async fn wait_async(&self) {
        let _p = self.semaphore.acquire_many(Self::max_permits()).await;
        self.close();
    }

    #[allow(dead_code)]
    pub(crate) fn is_closed(&self) -> bool {
        self.semaphore.is_closed()
    }
}
impl fmt::Debug for SyncGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncGroup")
            .field("notifiers", &self.num_active_notifiers())
            .finish()
    }
}

#[zenoh_macros::pub_visibility_if_internal]
pub(crate) type OnCancelHandlerId = usize;
struct OnCancelHandlers {
    handlers: HashMap<OnCancelHandlerId, Box<dyn FnOnce() -> ZResult<()> + Send + Sync>>,
    execution_finished_notifier: SyncGroupNotifier,
    next_handler_id: usize,
}

impl OnCancelHandlers {
    fn new(execution_finished_notifier: SyncGroupNotifier) -> Self {
        Self {
            handlers: HashMap::new(),
            execution_finished_notifier,
            next_handler_id: 0,
        }
    }

    fn add(
        &mut self,
        handler: impl FnOnce() -> ZResult<()> + Send + Sync + 'static,
    ) -> OnCancelHandlerId {
        let _ = self
            .handlers
            .insert(self.next_handler_id, Box::new(handler));
        self.next_handler_id += 1;
        self.next_handler_id - 1
    }

    fn remove(&mut self, id: OnCancelHandlerId) -> bool {
        self.handlers.remove(&id).is_some()
    }

    fn execute(self) -> (SyncGroupNotifier, ZResult<()>) {
        for (_, h) in self.handlers {
            if let Err(e) = (h)() {
                return (self.execution_finished_notifier, Err(e));
            }
        }
        (self.execution_finished_notifier, Ok(()))
    }

    fn num_handlers(&self) -> usize {
        self.handlers.len()
    }
}

/// A synchronization primitive that can be used to interrupt a get query.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let mut n = 0;
/// let cancellation_token = zenoh::cancellation::CancellationToken::default();
/// let queryable = session
///     .get("key/expression")
///     .callback_mut(move |reply| {n += 1;})
///     .cancellation_token(cancellation_token.clone())
///     .await
///     .unwrap();
///
/// // this call will interrupt query, after it returns it is guaranteed that callback will no longer be called
/// cancellation_token.cancel().await;
///  
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Clone)]
pub struct CancellationToken {
    on_cancel_handlers: Arc<Mutex<Option<OnCancelHandlers>>>,
    sync_group: SyncGroup,
    cancel_result: Arc<OnceLock<ZResult<()>>>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        let sync_group = SyncGroup::default();
        let on_cancel_handlers = OnCancelHandlers::new(
            sync_group
                .notifier()
                .expect("Notifier should be valid if sync group is not closed"),
        );
        Self {
            on_cancel_handlers: Arc::new(Mutex::new(Some(on_cancel_handlers))),
            sync_group,
            cancel_result: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct CancelResult(CancellationToken);

impl IntoFuture for CancelResult {
    type Output = ZResult<()>;

    type IntoFuture = BoxFuture<'static, ZResult<()>>;

    fn into_future(self) -> Self::IntoFuture {
        let v = self.0;
        let f = async move { v.cancel_inner_async().await };
        f.boxed()
    }
}

impl Resolvable for CancelResult {
    type To = ZResult<()>;
}

impl Wait for CancelResult {
    fn wait(self) -> Self::To {
        self.0.cancel_inner()
    }
}

impl CancellationToken {
    /// Interrupt all associated get queries.
    ///
    /// If the query callback is being executed, the call blocks until execution
    /// of callback is finished.
    ///
    /// Returns a future-like object that resolves to `ZResult<()>` in case of when awaited or when calling `.wait()`.
    /// In case of failure, some operations might not be cancelled.
    /// Once cancelled, all newly added get queries will cancel automatically.
    #[zenoh_macros::unstable_doc]
    pub fn cancel(&self) -> CancelResult {
        CancelResult(self.clone())
    }

    /// Returns true if token was cancelled. I.e. if [`CancellationToken::cancel`] was called.
    #[zenoh_macros::unstable_doc]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_result.get().is_some()
    }

    fn add_on_cancel_handler_inner<F>(&self, on_cancel: F) -> Result<OnCancelHandlerId, F>
    where
        F: FnOnce() -> ZResult<()> + Send + Sync + 'static,
    {
        let mut lk = self.on_cancel_handlers.lock().unwrap();
        match lk.deref_mut() {
            Some(actions) => Ok(actions.add(on_cancel)),
            None => Err(on_cancel),
        }
    }

    #[zenoh_macros::pub_visibility_if_internal]
    /// Register a handler to be called once [`CancellationToken::cancel`] is called.
    /// If cancel is already invoked, will return passed handler as a error, otherwise
    /// an id, which can be used to unregister the handler.
    pub(crate) fn add_on_cancel_handler<F>(&self, on_cancel: F) -> Result<OnCancelHandlerId, F>
    where
        F: FnOnce() -> ZResult<()> + Send + Sync + 'static,
    {
        self.add_on_cancel_handler_inner(on_cancel)
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) fn notifier(&self) -> Option<SyncGroupNotifier> {
        self.sync_group.notifier()
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) fn remove_on_cancel_handler(&self, id: OnCancelHandlerId) -> bool {
        self.on_cancel_handlers
            .lock()
            .unwrap()
            .deref_mut()
            .as_mut()
            .map(|h| h.remove(id))
            .unwrap_or(false)
    }

    fn execute_on_cancel_handlers(&self) -> Option<&ZResult<()>> {
        let mut lk = self.on_cancel_handlers.lock().unwrap();
        if let Some(actions) = std::mem::take(lk.deref_mut()) {
            drop(lk);
            let (notifier, res) = actions.execute();
            let out = Some(self.cancel_result.get_or_init(|| res));
            drop(notifier);
            out
        } else {
            None
        }
    }

    fn cancel_inner(&self) -> ZResult<()> {
        if let Some(res) = self.execute_on_cancel_handlers() {
            match res {
                Ok(_) => self.sync_group.wait(),
                Err(_) => self.sync_group.close(),
            };
        } else {
            self.sync_group.wait();
        }
        // self.cancel_result is guaranteed to be set after this point
        if let Some(res) = self.cancel_result.get() {
            match res {
                Ok(_) => Ok(()),
                Err(e) => bail!("Cancel failed: {e}"),
            }
        } else {
            // normally should never happen
            bail!("Cancellation token invariant is broken")
        }
    }

    async fn cancel_inner_async(&self) -> ZResult<()> {
        if let Some(res) = self.execute_on_cancel_handlers() {
            match res {
                Ok(_) => self.sync_group.wait_async().await,
                Err(_) => self.sync_group.close(),
            };
        } else {
            self.sync_group.wait_async().await;
        }
        // self.cancel_result is guaranteed to be set after this point
        if let Some(res) = self.cancel_result.get() {
            match res {
                Ok(_) => Ok(()),
                Err(e) => bail!("Cancel failed: {e}"),
            }
        } else {
            // normally should never happen
            bail!("Cancellation token invariant is broken")
        }
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationTokenInner")
            .field(
                "on_cancel_handlers",
                &self
                    .on_cancel_handlers
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|h| h.num_handlers())
                    .unwrap_or_default(),
            )
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

pub trait CancellationTokenBuilderTrait {
    fn cancellation_token(self, cancellation_token: CancellationToken) -> Self;
}

#[cfg(test)]
mod test {
    use std::{sync::atomic::AtomicUsize, time::Duration};

    use zenoh_core::Wait;

    use crate::cancellation::CancellationToken;

    #[test]
    fn concurrent_cancel() {
        let ct = CancellationToken::default();

        let n = std::sync::Arc::new(AtomicUsize::new(0));
        let n_clone = n.clone();
        let f = move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        };
        let _ = ct.add_on_cancel_handler(Box::new(f));

        let ct_clone = ct.clone();
        let n_clone = n.clone();
        let t = std::thread::spawn(move || {
            ct_clone.cancel().wait().unwrap();
            n_clone.load(std::sync::atomic::Ordering::SeqCst)
        });
        ct.cancel().wait().unwrap();
        assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(t.join().unwrap(), 1);
    }

    #[test]
    fn concurrent_cancel_with_err() {
        let ct = CancellationToken::default();

        let n = std::sync::Arc::new(AtomicUsize::new(0));
        let n_clone = n.clone();
        let f = move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            bail!("Error");
        };
        let _ = ct.add_on_cancel_handler(Box::new(f));

        let ct_clone = ct.clone();
        let n_clone = n.clone();
        let t = std::thread::spawn(move || {
            ct_clone.cancel().wait().unwrap_err();
            n_clone.load(std::sync::atomic::Ordering::SeqCst)
        });
        ct.cancel().wait().unwrap_err();
        assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(t.join().unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_cancel_async() {
        let ct = CancellationToken::default();

        let n = std::sync::Arc::new(AtomicUsize::new(0));
        let n_clone = n.clone();
        let f = move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        };
        let _ = ct.add_on_cancel_handler(Box::new(f));

        let ct_clone = ct.clone();
        let n_clone = n.clone();
        let t = tokio::spawn(async move {
            ct_clone.cancel().await.unwrap();
            n_clone.load(std::sync::atomic::Ordering::SeqCst)
        });
        ct.cancel().await.unwrap();
        assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(t.await.unwrap(), 1);
    }
}
