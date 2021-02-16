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
use async_std::sync::{Arc, RwLock};

use super::proto::ZenohMessage;
use super::session::defaults::QUEUE_PRIO_DATA;
use super::{Scheduling, SessionTransportLink};

use async_trait::async_trait;
use zenoh_util::zasyncread;

pub(crate) struct FirstMatch;

impl FirstMatch {
    pub(crate) fn new() -> FirstMatch {
        FirstMatch
    }
}

#[async_trait]
impl Scheduling for FirstMatch {
    async fn schedule(&self, msg: ZenohMessage, links: &Arc<RwLock<Vec<SessionTransportLink>>>) {
        let guard = zasyncread!(links);
        for cl in guard.iter() {
            let link = cl.get_link();
            if msg.is_reliable() && link.is_reliable() {
                cl.schedule_zenoh_message(msg, QUEUE_PRIO_DATA).await;
                return;
            } else if !msg.is_reliable() && !link.is_reliable() {
                cl.schedule_zenoh_message(msg, QUEUE_PRIO_DATA).await;
                return;
            }
        }
        match guard.get(0) {
            Some(cl) => cl.schedule_zenoh_message(msg, QUEUE_PRIO_DATA).await,
            None => log::trace!("Message dropped because the session has no links: {}", msg),
        }
    }
}
