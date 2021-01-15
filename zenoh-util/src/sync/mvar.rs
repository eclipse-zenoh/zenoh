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
use super::Condition;
use crate::zasynclock;
use async_std::sync::Mutex;

pub struct Mvar<T> {
    inner: Mutex<Option<T>>,
    cond_put: Condition,
    cond_take: Condition,
}

impl<T> Mvar<T> {
    pub fn new() -> Mvar<T> {
        Mvar {
            inner: Mutex::new(None),
            cond_put: Condition::new(),
            cond_take: Condition::new(),
        }
    }

    pub async fn take(&self) -> T {
        loop {
            let mut guard = zasynclock!(self.inner);
            if let Some(inner) = guard.take() {
                drop(guard);
                self.cond_put.notify_one();
                return inner;
            }
            self.cond_take.wait(guard).await;
        }
    }

    pub async fn put(&self, inner: T) {
        loop {
            let mut guard = zasynclock!(self.inner);
            if guard.is_some() {
                self.cond_put.wait(guard).await;
            } else {
                *guard = Some(inner);
                drop(guard);
                self.cond_take.notify_one();
                return;
            }
        }
    }
}

impl<T> Default for Mvar<T> {
    fn default() -> Self {
        Self::new()
    }
}
