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
use super::core::{Channel, PeerId, ZInt};
use super::proto::{Close, Frame, FramePayload, SessionBody, SessionMessage};
use super::{Link, SessionTransport};
use async_std::task;
use zenoh_util::zread;

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
macro_rules! zcallback {
    ($transport:expr, $msg:expr) => {
        let callback = zread!($transport.callback).clone();
        match callback.as_ref() {
            Some(callback) => {
                let _ = callback.handle_message($msg);
            }
            None => {
                log::trace!(
                    "Session: {}. No callback available, dropping message: {}",
                    $transport.pid,
                    $msg
                );
            }
        }
    };
}

macro_rules! zreceiveframe {
    ($transport:expr, $link:expr, $guard:expr, $sn:expr, $payload:expr) => {
        let precedes = match $guard.sn.precedes($sn) {
            Ok(precedes) => precedes,
            Err(e) => {
                log::warn!(
                    "Session: {}. Invalid SN in frame: {}. Closing the session.",
                    $transport.pid,
                    e
                );
                // Drop the guard before closing the session
                drop($guard);
                // Stop the link
                let _ = $transport.stop_rx($link);
                let _ = $transport.stop_tx($link);
                // Delete the whole session
                let tr = $transport.clone();
                task::spawn(async move {
                    tr.delete().await;
                });
                // Close the link
                return;
            }
        };
        if !precedes {
            log::warn!(
                "Session: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                $transport.pid,
                $sn,
                $guard.sn.get()
            );
            // Drop the fragments if needed
            if !$guard.defrag_buffer.is_empty() {
                $guard.defrag_buffer.clear();
            }
            // Keep reading
            return;
        }

        // Set will always return OK because we have already checked
        // with precedes() that the sn has the right resolution
        let _ = $guard.sn.set($sn);
        match $payload {
            FramePayload::Fragment { buffer, is_final } => {
                if $guard.defrag_buffer.is_empty() {
                    let _ = $guard.defrag_buffer.sync($sn);
                }
                let res = $guard.defrag_buffer.push($sn, buffer);
                if let Err(e) = res {
                    log::trace!(
                        "Session: {}. Defragmentation error: {:?}",
                        $transport.pid,
                        e
                    );
                    $guard.defrag_buffer.clear();
                    return;
                }

                if is_final {
                    let msg = match $guard.defrag_buffer.defragment() {
                        Some(msg) => msg,
                        None => {
                            log::trace!("Session: {}. Defragmentation error.", $transport.pid);
                            return;
                        }
                    };
                    zcallback!($transport, msg);
                }
            }
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    zcallback!($transport, msg);
                }
            }
        }
    };
}

impl SessionTransport {
    fn handle_close(&self, link: &Link, pid: Option<PeerId>, reason: u8, link_only: bool) {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::warn!(
                    "Received an invalid Close on link {} from peer {} with reason: {}. Ignoring.",
                    link,
                    pid,
                    reason
                );
                return;
            }
        }

        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        let c_transport = self.clone();
        let c_link = link.clone();
        task::spawn(async move {
            if link_only {
                let _ = c_transport.del_link(&c_link).await;
            } else {
                c_transport.delete().await;
            }
        });
    }

    /*************************************/
    /*   MESSAGE RECEIVED FROM THE LINK  */
    /*************************************/
    fn handle_reliable_frame(&self, link: &Link, sn: ZInt, payload: FramePayload) {
        // @TODO: Implement the reordering and reliability. Wait for missing messages.
        let mut guard = zlock!(self.rx_reliable);
        zreceiveframe!(self, link, guard, sn, payload);
    }

    fn handle_best_effort_frame(&self, link: &Link, sn: ZInt, payload: FramePayload) {
        let mut guard = zlock!(self.rx_best_effort);
        zreceiveframe!(self, link, guard, sn, payload);
    }

    #[inline(always)]
    pub(super) fn receive_message(&self, message: SessionMessage, link: &Link) {
        // Process the received message
        match message.body {
            SessionBody::Frame(Frame { ch, sn, payload }) => match ch {
                Channel::Reliable => self.handle_reliable_frame(link, sn, payload),
                Channel::BestEffort => self.handle_best_effort_frame(link, sn, payload),
            },
            SessionBody::Close(Close {
                pid,
                reason,
                link_only,
            }) => self.handle_close(link, pid, reason, link_only),
            _ => {
                log::trace!(
                    "Session: {}. Message handling not implemented: {:?}",
                    self.pid,
                    message
                );
            }
        }
    }
}
