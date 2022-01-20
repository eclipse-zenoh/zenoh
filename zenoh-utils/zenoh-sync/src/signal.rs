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

#[derive(Clone)]
pub struct Signal {
    event: Arc<Event>,
}

impl Signal {
    pub fn new() -> Self {
        let event = Arc::new(Event::new());
        Signal { event }
    }

    pub fn trigger(&self) {
        self.event.notify_additional(usize::MAX);
    }

    pub async fn wait(&self) {
        let listener = self.event.listen();
        listener.await;
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}
