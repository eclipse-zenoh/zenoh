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
use std::sync::{
    atomic::{AtomicBool, Ordering::*},
    Arc,
};

use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct Signal {
    shared: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    semaphore: Semaphore,
    triggered: AtomicBool,
}

impl Signal {
    pub fn new() -> Self {
        Signal {
            shared: Arc::new(Inner {
                semaphore: Semaphore::new(0),
                triggered: AtomicBool::new(false),
            }),
        }
    }

    pub fn trigger(&self) {
        let result = self
            .shared
            .triggered
            .compare_exchange(false, true, AcqRel, Acquire);

        if result.is_ok() {
            // The maximum # of permits is defined in tokio doc.
            // https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html#method.add_permits
            self.shared.semaphore.add_permits(usize::MAX >> 3);
        }
    }

    pub fn is_triggered(&self) -> bool {
        self.shared.triggered.load(Acquire)
    }

    pub async fn wait(&self) {
        if !self.is_triggered() {
            let _ = self.shared.semaphore.acquire().await;
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signal_test() {
        let signal = Signal::new();

        // spawn publisher
        let r#pub = tokio::task::spawn({
            let signal = signal.clone();

            async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                signal.trigger();
                signal.trigger(); // second trigger should not break
            }
        });

        // spawn subscriber that waits immediately
        let fast_sub = tokio::task::spawn({
            let signal = signal.clone();

            async move {
                signal.wait().await;
            }
        });

        // spawn subscriber that waits after the publisher triggers the signal
        let slow_sub = tokio::task::spawn({
            let signal = signal.clone();

            async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                signal.wait().await;
            }
        });

        // check that the slow subscriber does not half
        let result = tokio::time::timeout(
            Duration::from_millis(50000),
            futures::future::join3(r#pub, fast_sub, slow_sub),
        )
        .await;
        assert!(result.is_ok());

        // verify if signal is in triggered state
        assert!(signal.is_triggered());
    }
}
