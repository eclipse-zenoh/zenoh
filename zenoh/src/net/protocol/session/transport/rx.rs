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
use super::core::{Conduit, PeerId, Reliability, ZInt};
use super::proto::{
    Close, Frame, FramePayload, KeepAlive, SessionBody, SessionMessage, ZenohMessage,
};
use super::{Link, SessionTransport, SessionTransportChannelRx};
use async_std::task;
use std::sync::MutexGuard;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zerror2, zread};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl SessionTransport {
    #[allow(unused_mut)]
    fn trigger_callback(&self, mut msg: ZenohMessage) -> ZResult<()> {
        let callback = zread!(self.callback).clone();
        match callback.as_ref() {
            Some(callback) => {
                #[cfg(feature = "zero-copy")]
                let _ = msg.map_to_shmbuf(self.manager.shmr.clone())?;
                callback.handle_message(msg)
            }
            None => {
                log::debug!(
                    "Session: {}. No callback available, dropping message: {}",
                    self.pid,
                    msg
                );
                Ok(())
            }
        }
    }

    fn handle_close(
        &self,
        link: &Link,
        pid: Option<PeerId>,
        reason: u8,
        link_only: bool,
    ) -> ZResult<()> {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::debug!(
                    "Received an invalid Close on link {} from peer {} with reason: {}. Ignoring.",
                    link,
                    pid,
                    reason
                );
                return Ok(());
            }
        }

        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            if link_only {
                let _ = c_transport.del_link(&c_link).await;
            } else {
                let _ = c_transport.delete().await;
            }
        });

        Ok(())
    }

    fn handle_frame(
        &self,
        sn: ZInt,
        payload: FramePayload,
        mut guard: MutexGuard<'_, SessionTransportChannelRx>,
    ) -> ZResult<()> {
        let precedes = guard.sn.precedes(sn)?;
        if !precedes {
            log::debug!(
                "Session: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.pid,
                sn,
                guard.sn.get()
            );
            // Drop the fragments if needed
            if !guard.defrag.is_empty() {
                guard.defrag.clear();
            }
            // Keep reading
            return Ok(());
        }

        // Set will always return OK because we have already checked
        // with precedes() that the sn has the right resolution
        let _ = guard.sn.set(sn);
        match payload {
            FramePayload::Fragment { buffer, is_final } => {
                if guard.defrag.is_empty() {
                    let _ = guard.defrag.sync(sn);
                }
                guard.defrag.push(sn, buffer)?;
                if is_final {
                    // When zero-copy feature is disabled, msg does not need to be mutable
                    let msg = guard.defrag.defragment().ok_or_else(|| {
                        let e = format!("Session: {}. Defragmentation error.", self.pid);
                        zerror2!(ZErrorKind::InvalidMessage { descr: e })
                    })?;
                    self.trigger_callback(msg)
                } else {
                    Ok(())
                }
            }
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    self.trigger_callback(msg)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn receive_message(&self, msg: SessionMessage, link: &Link) -> ZResult<()> {
        // Process the received message
        match msg.body {
            SessionBody::Frame(Frame {
                channel,
                sn,
                payload,
            }) => {
                let c = if self.is_qos() {
                    &self.conduit_rx[channel.conduit as usize]
                } else if channel.conduit == Conduit::default() {
                    &self.conduit_rx[0]
                } else {
                    let e = format!(
                        "Session: {}. Unknown conduit: {:?}.",
                        self.pid, channel.conduit
                    );
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                };

                match channel.reliability {
                    Reliability::Reliable => self.handle_frame(sn, payload, zlock!(c.reliable)),
                    Reliability::BestEffort => {
                        self.handle_frame(sn, payload, zlock!(c.best_effort))
                    }
                }
            }
            SessionBody::Close(Close {
                pid,
                reason,
                link_only,
            }) => self.handle_close(link, pid, reason, link_only),
            SessionBody::KeepAlive(KeepAlive { .. }) => Ok(()),
            _ => {
                log::debug!(
                    "Session: {}. Message handling not implemented: {:?}",
                    self.pid,
                    msg
                );
                Ok(())
            }
        }
    }
}
