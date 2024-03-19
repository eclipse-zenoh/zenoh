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

use std::future::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
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
    /// Spawns a task (similarly to task::spawn) that can be later terminated by call to terminate_all()
    /// Task output is ignored
    pub fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
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

    /// Spawns a task using a specified runtime (similarly to Runtime::spawn) that can be later terminated by call to terminate_all().
    /// Task output is ignored.
    pub fn spawn_with_rt<F, T>(&self, rt: ZRuntime, future: F) -> JoinHandle<()>
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

    /// Spawns a task that can be cancelled via cancelling a token obtained by get_cancellation_token().
    /// It can be later terminated by call to terminate_all().
    pub fn spawn_cancellable<F, T>(&self, future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.tracker.spawn(future)
    }

    /// Spawns a task that can be cancelled via cancelling a token obtained by get_cancellation_token() using a specified runtime.
    /// It can be later terminated by call to terminate_all().
    pub fn spawn_cancellable_with_rt<F, T>(&self, rt: ZRuntime, future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.tracker.spawn_on(future, &rt)
    }

    /// Terminates all prevously spawned tasks
    pub fn terminate_all(&self) {
        self.tracker.close();
        self.token.cancel();
        let tracker = self.tracker.clone();
        futures::executor::block_on(async move { tracker.wait().await });
    }

    /// Terminates all prevously spawned tasks
    pub async fn terminate_all_async(&self) {
        self.tracker.close();
        self.token.cancel();
        self.tracker.wait().await;
    }
}
