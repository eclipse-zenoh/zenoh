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
macro_rules! zclose {
    ($transport:expr, $link:expr) => {
        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = $transport.stop_rx($link);
        let _ = $transport.stop_tx($link);
        // Delete the whole session
        let tr = $transport.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            let _ = tr.delete().await;
        });
    };
}
macro_rules! zcallback {
    ($transport:expr, $link:expr, $msg:expr) => {
        let callback = zread!($transport.callback).clone();
        match callback.as_ref() {
            Some(callback) => {
                #[cfg(feature = "zero-copy")]
                {
                    // First, try in read mode allowing concurrenct lookups
                    let r_guard = zread!($transport.manager.shmr);
                    let res = $msg.try_map_to_shmbuf(&*r_guard).or_else(|_| {
                        // Next, try in write mode to eventual link the remote shm
                        drop(r_guard);
                        let mut w_guard = zwrite!($transport.manager.shmr);
                        $msg.map_to_shmbuf(&mut *w_guard)
                    });
                    if let Err(e) = res {
                        log::trace!(
                            "Session: {}. Error from SharedMemory: {}. Closing session.",
                            $transport.pid,
                            e
                        );
                        zclose!($transport, $link);
                    }
                }

                let res = callback.handle_message($msg);
                if let Err(e) = res {
                    log::trace!(
                        "Session: {}. Error from callback: {}. Closing session.",
                        $transport.pid,
                        e
                    );
                    zclose!($transport, $link);
                }
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
                // Stop now rx and tx tasks before doing the proper cleanup
                zclose!($transport, $link);
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
                    // When zero-copy feature is disabled, msg does not need to be mutable
                    #[allow(unused_mut)]
                    let mut msg = match $guard.defrag_buffer.defragment() {
                        Some(msg) => msg,
                        None => {
                            log::trace!("Session: {}. Defragmentation error.", $transport.pid);
                            return;
                        }
                    };
                    zcallback!($transport, $link, msg);
                }
            }
            FramePayload::Messages { mut messages } => {
                // When zero-copy feature is disabled, msg does not need to be mutable
                #[allow(unused_mut)]
                for mut msg in messages.drain(..) {
                    zcallback!($transport, $link, msg);
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
