//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::MutexGuard;
use event_listener::Event;

/// This is a Condition Variable similar to that provided by POSIX.
/// As for POSIX condition variables, this assumes that a mutex is
/// properly used to coordinate behaviour. In other terms there should
/// not be race condition on [notify](Condition::notify).
///
pub struct Condition {
    event: Event,
}

impl Default for Condition {
    fn default() -> Condition {
        Condition {
            event: Event::new(),
        }
    }
}

impl Condition {
    /// Creates a new condition variable with a given capacity.
    /// The capacity indicates the maximum number of tasks that
    /// may be waiting on the condition.
    pub fn new() -> Condition {
        Condition::default()
    }

    /// Waits for the condition to be notified
    #[inline]
    pub async fn wait<T>(&self, guard: MutexGuard<'_, T>) {
        let listener = self.event.listen();
        drop(guard);
        listener.await;
    }

    /// Notifies one pending listener
    #[inline]
    pub fn notify_one(&self) {
        self.event.notify_additional_relaxed(1);
    }

    /// Notifies all pending listeners
    #[inline]
    pub fn notify_all(&self) {
        self.event.notify_additional_relaxed(usize::MAX);
    }
}
