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
use super::SessionTransport;
use async_std::sync::Weak;
use async_trait::async_trait;
use zenoh_util::collections::Timed;

/*************************************/
/*        SESSION LEASE EVENT        */
/*************************************/
pub(super) struct SessionLeaseEvent {
    ch: Weak<SessionTransport>,
}

impl SessionLeaseEvent {
    pub(super) fn new(ch: Weak<SessionTransport>) -> SessionLeaseEvent {
        SessionLeaseEvent { ch }
    }
}

#[async_trait]
impl Timed for SessionLeaseEvent {
    async fn run(&mut self) {
        if let Some(ch) = self.ch.upgrade() {
            let links = ch.get_links().await;
            if links.is_empty() {
                log::warn!("Session has expired with peer: {}", ch.get_pid());
                // The last link has expired, close the whole session
                let _ = ch.delete().await;
            }
        }
    }
}
