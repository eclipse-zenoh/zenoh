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
    collections::VecDeque,
    future::IntoFuture,
    ops::DerefMut,
    sync::{Arc, Mutex, OnceLock},
};

use futures::{future::BoxFuture, FutureExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct SyncGroupNotifier(Arc<OwnedSemaphorePermit>);

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

    pub(crate) fn notifier(&self) -> Option<SyncGroupNotifier> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|p| SyncGroupNotifier(Arc::new(p)))
    }

    pub(crate) fn close(&self) {
        self.semaphore.close();
    }

    pub(crate) fn wait(&self) {
        let s = self.semaphore.clone();
        let _p = futures::executor::block_on(s.acquire_many(Self::max_permits()));
        self.close();
    }
    pub(crate) async fn wait_async(&self) {
        let _p = self.semaphore.acquire_many(Self::max_permits()).await;
        self.close();
    }

    #[allow(dead_code)]
    pub(crate) fn is_closed(&self) -> bool {
        self.semaphore.is_closed()
    }
}

pub(crate) type OnCancel = Box<dyn FnOnce() -> ZResult<()> + Send + Sync>;
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
    on_cancel_handlers: Arc<Mutex<Option<VecDeque<OnCancel>>>>,
    sync_group: SyncGroup,
    cancel_result: Arc<OnceLock<ZResult<()>>>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        let sync_group = SyncGroup::default();
        let notifier = sync_group.notifier();
        let on_cancel_sync: OnCancel = Box::new(move || {
            // fake handler for concurrent cancel synchronization
            drop(notifier);
            Ok(())
        });
        Self {
            on_cancel_handlers: Arc::new(Mutex::new(Some(VecDeque::from([on_cancel_sync])))),
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
    /// Interrupt all associated get queries. If the query callback is being executed, the call blocks until execution
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

    pub(crate) fn add_on_cancel_handler(&self, on_cancel: OnCancel) {
        let mut lk = self.on_cancel_handlers.lock().unwrap();
        match lk.deref_mut() {
            Some(actions) => actions.push_front(on_cancel),
            None => {
                if let Err(err) = (on_cancel)() {
                    // ensure sync group does not wait indefinitely if we just failed to cancel the notifier
                    let _ = self.cancel_result.set(Err(err));
                    self.sync_group.close();
                }
            }
        };
    }

    pub(crate) fn notifier(&self) -> Option<SyncGroupNotifier> {
        self.sync_group.notifier()
    }

    fn execute_on_cancel_handlers(&self) -> ZResult<()> {
        let actions =
            std::mem::take(self.on_cancel_handlers.lock().unwrap().deref_mut()).unwrap_or_default();
        for a in actions {
            (a)()?;
        }
        Ok(())
    }

    fn cancel_inner(&self) -> ZResult<()> {
        let res = self.execute_on_cancel_handlers();
        match res {
            Ok(_) => self.sync_group.wait(),
            Err(_) => self.sync_group.close(),
        };
        match self.cancel_result.get_or_init(|| res).as_ref() {
            Ok(_) => Ok(()),
            Err(e) => bail!("Cancel failed: {e}"),
        }
    }

    async fn cancel_inner_async(&self) -> ZResult<()> {
        let res = self.execute_on_cancel_handlers();
        match res {
            Ok(_) => self.sync_group.wait_async().await,
            Err(_) => self.sync_group.close(),
        };
        match self.cancel_result.get_or_init(|| res).as_ref() {
            Ok(_) => Ok(()),
            Err(e) => bail!("Cancel failed: {e}"),
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
                    .map(|h| h.len())
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
        ct.add_on_cancel_handler(Box::new(f));

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
        ct.add_on_cancel_handler(Box::new(f));

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
