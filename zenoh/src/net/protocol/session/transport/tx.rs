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
use super::proto::ZenohMessage;
use super::session::defaults::ZN_QUEUE_PRIO_DATA;
use super::SessionTransport;
use zenoh_util::zread;

impl SessionTransport {
    #[inline(always)]
    pub(super) fn schedule_first_fit(&self, msg: ZenohMessage) {
        macro_rules! zpush {
            ($guard:expr, $pipeline:expr, $msg:expr) => {
                // Drop the guard before the push_zenoh_message since
                // the link could be congested and this operation could
                // block for fairly long time
                drop($guard);
                $pipeline.push_zenoh_message($msg, ZN_QUEUE_PRIO_DATA);
                return;
            };
        }

        let guard = zread!(self.links);
        // First try to find the best match between msg and link reliability
        for sl in guard.iter() {
            if let Some(pipeline) = sl.get_pipeline() {
                let link = sl.get_link();
                if msg.is_reliable() && link.is_reliable() {
                    zpush!(guard, pipeline, msg);
                }
                if !msg.is_reliable() && !link.is_reliable() {
                    zpush!(guard, pipeline, msg);
                }
            }
        }

        // No best match found, take the first available link
        for sl in guard.iter() {
            if let Some(pipeline) = sl.get_pipeline() {
                zpush!(guard, pipeline, msg);
            }
        }

        // No Link found
        log::trace!("Message dropped because the session has no links: {}", msg);
    }
}
