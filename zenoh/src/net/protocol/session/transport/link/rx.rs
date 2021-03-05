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
use super::core::PeerId;
use super::proto::{Close, SessionBody, SessionMessage};
use super::SessionTransportLink;

/*************************************/
/*              LINK RX              */
/*************************************/
impl SessionTransportLink {
    async fn handle_close(&self, pid: Option<PeerId>, reason: u8, link_only: bool) {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.transport.pid {
                log::warn!(
                    "Received an invalid Close on link {} from peer {} with reason: {}. Ignoring.",
                    self.inner,
                    pid,
                    reason
                );
                return;
            }
        }

        let _ = self.transport.del_link(&self.inner).await;

        if !link_only {
            self.transport.delete().await;
        }
    }

    pub(super) async fn receive_message(&self, message: SessionMessage) {
        log::trace!(
            "Received from peer {} on link {}: {:?}",
            self.transport.pid,
            self.inner,
            message
        );

        // Process the received message
        match message.body {
            SessionBody::Close(Close {
                pid,
                reason,
                link_only,
            }) => self.handle_close(pid, reason, link_only).await,
            SessionBody::KeepAlive { .. } => {}
            SessionBody::Ping { .. }
            | SessionBody::Pong { .. }
            | SessionBody::Sync { .. }
            | SessionBody::AckNack { .. } => {
                log::trace!("Handling of message not yet implemented: {:?}", message);
            }
            _ => self.transport.receive_message(message).await,
        }
    }
}
