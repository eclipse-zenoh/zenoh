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
    future::IntoFuture,
    sync::{atomic::AtomicBool, Mutex},
};

use flume::{Receiver, Sender};
use futures::{future::BoxFuture, FutureExt};
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct SyncGroupNotifier(Sender<()>);
#[derive(Clone)]
pub(crate) struct SyncGroupReceiver(Receiver<()>);

impl SyncGroupReceiver {
    pub(crate) fn wait(self) {
        self.0.recv().unwrap_err();
    }
    pub(crate) async fn wait_async(self) {
        self.0.recv_async().await.unwrap_err();
    }
}

pub(crate) fn create_sync_group_receiver_notifier_pair() -> (SyncGroupNotifier, SyncGroupReceiver) {
    let (notifier, receiver) = flume::bounded(0);
    (SyncGroupNotifier(notifier), SyncGroupReceiver(receiver))
}

pub(crate) type OnCancel = Box<dyn FnOnce() -> ZResult<()> + Send + Sync>;

struct OnCancelHandler {
    receiver: SyncGroupReceiver,
    on_cancel: OnCancel,
}

impl OnCancelHandler {
    fn cancel(self) -> ZResult<()> {
        (self.on_cancel)()?;
        self.receiver.wait();
        Ok(())
    }

    async fn cancel_async(self) -> ZResult<()> {
        (self.on_cancel)()?;
        self.receiver.wait_async().await;
        Ok(())
    }
}

#[derive(Default)]
struct CancellationTokenInner {
    on_cancel_handlers: Mutex<Vec<OnCancelHandler>>,
    is_cancelled: AtomicBool,
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
#[derive(Clone, Default, Debug)]
pub struct CancellationToken {
    inner: std::sync::Arc<CancellationTokenInner>,
}

#[derive(Default)]
pub struct CancelResult(Vec<OnCancelHandler>);

impl IntoFuture for CancelResult {
    type Output = ZResult<()>;

    type IntoFuture = BoxFuture<'static, ZResult<()>>;

    fn into_future(self) -> Self::IntoFuture {
        let v = self.0;
        let f = async {
            for v in v {
                v.cancel_async().await?;
            }
            Ok(())
        };
        f.boxed()
    }
}

impl Resolvable for CancelResult {
    type To = ZResult<()>;
}

impl Wait for CancelResult {
    fn wait(self) -> Self::To {
        for c in self.0 {
            c.cancel()?;
        }
        Ok(())
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
        match self.inner.is_cancelled.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(_) => {
                let mut lk = self.inner.on_cancel_handlers.lock().unwrap();
                CancelResult(std::mem::take(lk.as_mut()))
            }
            Err(_) => CancelResult::default(),
        }
    }

    /// Returns true if token was cancelled. I.e. if [`CancellationToken::cancel`] was called.
    #[zenoh_macros::unstable_doc]
    pub fn is_cancelled(&self) -> bool {
        self.inner
            .is_cancelled
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn add_on_cancel_handler(&self, receiver: SyncGroupReceiver, on_cancel: OnCancel) {
        let on_cancel_handler = OnCancelHandler {
            receiver,
            on_cancel,
        };
        // acquire lock to ensure that cancel handlers will not be moved out in the meantime by call to cancel and that we
        // do not end up with a stale on_cancel_handler
        let mut lk = self.inner.on_cancel_handlers.lock().unwrap();
        if self.is_cancelled() {
            drop(lk);
            let _ = on_cancel_handler.cancel();
        } else {
            lk.push(on_cancel_handler);
        }
    }
}

impl fmt::Debug for CancellationTokenInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationTokenInner")
            .field(
                "on_cancel_handlers",
                &self.on_cancel_handlers.lock().unwrap().len(),
            )
            .field(
                "is_cancelled",
                &self.is_cancelled.load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

pub trait CancellationTokenBuilderTrait {
    fn cancellation_token(self, cancellation_token: CancellationToken) -> Self;
}
