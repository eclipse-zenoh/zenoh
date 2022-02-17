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
use async_std::sync::Arc;
use event_listener::Event;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::*;

#[derive(Debug, Clone)]
pub struct Signal {
    shared: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    event: Event,
    triggered: AtomicBool,
}

impl Signal {
    pub fn new() -> Self {
        Signal {
            shared: Arc::new(Inner {
                event: Event::new(),
                triggered: AtomicBool::new(false),
            }),
        }
    }

    pub fn trigger(&self) {
        self.shared.triggered.store(true, Release);
        self.shared.event.notify_additional(usize::MAX);
    }

    pub fn is_triggered(&self) -> bool {
        self.shared.triggered.load(Acquire)
    }

    pub async fn wait(&self) {
        if !self.is_triggered() {
            let listener = self.shared.event.listen();
            listener.await;
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}
