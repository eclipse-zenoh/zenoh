//
// Copyright (c) 2024 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)

use futures::future::FutureExt;
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use zenoh_core::{ResolveFuture, SyncResolve};
use zenoh_runtime::ZRuntime;

#[derive(Clone)]
pub struct TaskController {
    tracker: TaskTracker,
    token: CancellationToken,
}

impl Default for TaskController {
    fn default() -> Self {
        TaskController {
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
        }
    }
}

impl TaskController {
    /// Spawns a task that can be later terminated by call to [`TaskController::terminate_all()`].
    /// Task output is ignored.
    pub fn spawn_abortable<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = self.token.child_token();
        let task = async move {
            tokio::select! {
                _ = token.cancelled() => {},
                _ = future => {}
            }
        };
        self.tracker.spawn(task)
    }

    /// Spawns a task using a specified runtime that can be later terminated by call to [`TaskController::terminate_all()`].
    /// Task output is ignored.
    pub fn spawn_abortable_with_rt<F, T>(&self, rt: ZRuntime, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = self.token.child_token();
        let task = async move {
            tokio::select! {
                _ = token.cancelled() => {},
                _ = future => {}
            }
        };
        self.tracker.spawn_on(task, &rt)
    }

    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.token.child_token()
    }

    /// Spawns a task that can be cancelled via cancellation of a token obtained by [`TaskController::get_cancellation_token()`],
    /// or that can run to completion in finite amount of time.
    /// It can be later terminated by call to [`TaskController::terminate_all()`].
    pub fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.tracker.spawn(future.map(|_f| ()))
    }

    /// Spawns a task that can be cancelled via cancellation of a token obtained by [`TaskController::get_cancellation_token()`],
    /// or that can run to completion in finite amount of time, using a specified runtime.
    /// It can be later aborted by call to [`TaskController::terminate_all()`].
    pub fn spawn_with_rt<F, T>(&self, rt: ZRuntime, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.tracker.spawn_on(future.map(|_f| ()), &rt)
    }

    /// Attempts tp terminate all previously spawned tasks
    /// The caller must ensure that all tasks spawned with [`TaskController::spawn()`]
    /// or [`TaskController::spawn_with_rt()`] can yield in finite amount of time either because they will run to completion
    /// or due to cancellation of token acquired via [`TaskController::get_cancellation_token()`].
    /// Tasks spawned with [`TaskController::spawn_abortable()`] or [`TaskController::spawn_abortable_with_rt()`] will be aborted (i.e. terminated upon next await call).
    /// The call blocks until all tasks yield or timeout duration expires.
    /// Returns 0 in case of success, number of non terminated tasks otherwise.
    pub fn terminate_all(&self, timeout: Duration) -> usize {
        ResolveFuture::new(async move { self.terminate_all_async(timeout).await }).res_sync()
    }

    /// Async version of [`TaskController::terminate_all()`].
    pub async fn terminate_all_async(&self, timeout: Duration) -> usize {
        self.tracker.close();
        self.token.cancel();
        if tokio::time::timeout(timeout, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::error!("Failed to terminate {} tasks", self.tracker.len());
            return self.tracker.len();
        }
        0
    }
}

pub struct TerminatableTask {
    handle: JoinHandle<()>,
    token: CancellationToken,
}

impl TerminatableTask {
    pub fn create_cancellation_token() -> CancellationToken {
        CancellationToken::new()
    }

    /// Spawns a task that can be later terminated by [`TerminatableTask::terminate()`].
    /// Prior to termination attempt the specified cancellation token will be cancelled.
    pub fn spawn<F, T>(rt: ZRuntime, future: F, token: CancellationToken) -> TerminatableTask
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        TerminatableTask {
            handle: rt.spawn(future.map(|_f| ())),
            token,
        }
    }

    /// Spawns a task that can be later aborted by [`TerminatableTask::terminate()`].
    pub fn spawn_abortable<F, T>(rt: ZRuntime, future: F) -> TerminatableTask
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = CancellationToken::new();
        let token2 = token.clone();
        let task = async move {
            tokio::select! {
                _ = token2.cancelled() => {},
                _ = future => {}
            }
        };

        TerminatableTask {
            handle: rt.spawn(task),
            token,
        }
    }

    /// Attempts to terminate the task.
    /// Returns true if task completed / aborted within timeout duration, false otherwise.
    pub fn terminate(self, timeout: Duration) -> bool {
        ResolveFuture::new(async move { self.terminate_async(timeout).await }).res_sync()
    }

    /// Async version of [`TerminatableTask::terminate()`].
    pub async fn terminate_async(self, timeout: Duration) -> bool {
        self.token.cancel();
        if tokio::time::timeout(timeout, self.handle).await.is_err() {
            tracing::error!("Failed to terminate the task");
            return false;
        };
        true
    }
}
