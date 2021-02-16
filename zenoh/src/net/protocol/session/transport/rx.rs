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
use super::core::{Channel, ZInt};
use super::proto::{Frame, FramePayload, SessionBody, SessionMessage};
use super::SessionTransport;
use zenoh_util::{zasynclock, zasyncread};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
macro_rules! zcallback {
    ($transport:expr, $msg:expr) => {
        let callback = zasyncread!($transport.callback).clone();
        match callback.as_ref() {
            Some(callback) => {
                let _ = callback.handle_message($msg).await;
            }
            None => {
                log::trace!("No callback available, dropping message: {}", $msg);
            }
        }
    };
}

macro_rules! zreceiveframe {
    ($transport:expr, $guard:expr, $sn:expr, $payload:expr) => {
        let precedes = match $guard.sn.precedes($sn) {
            Ok(precedes) => precedes,
            Err(e) => {
                log::warn!(
                    "Invalid SN in frame: {}. Closing the session with peer: {}",
                    e,
                    $transport.pid
                );
                // Drop the guard before closing the session
                drop($guard);
                // Delete the whole session
                $transport.delete().await;
                // Close the link
                return;
            }
        };
        if !precedes {
            log::warn!(
                "Frame with invalid SN dropped: {}. Expected: {}",
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
                    // return Action::Read;
                    return;
                }

                if is_final {
                    let msg = match $guard.defrag_buffer.defragment() {
                        Some(msg) => msg,
                        None => {
                            log::trace!("Session: {}. Defragmentation error.", $transport.pid);
                            // return Action::Read;
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
    /*************************************/
    /*   MESSAGE RECEIVED FROM THE LINK  */
    /*************************************/
    async fn handle_reliable_frame(&self, sn: ZInt, payload: FramePayload) {
        // @TODO: Implement the reordering and reliability. Wait for missing messages.
        let mut guard = zasynclock!(self.rx_reliable);
        zreceiveframe!(self, guard, sn, payload);
    }

    async fn handle_best_effort_frame(&self, sn: ZInt, payload: FramePayload) {
        let mut guard = zasynclock!(self.rx_best_effort);
        zreceiveframe!(self, guard, sn, payload);
    }

    pub(super) async fn receive_message(&self, message: SessionMessage) {
        // Process the received message
        match message.body {
            SessionBody::Frame(Frame { ch, sn, payload }) => match ch {
                Channel::Reliable => self.handle_reliable_frame(sn, payload).await,
                Channel::BestEffort => self.handle_best_effort_frame(sn, payload).await,
            },
            SessionBody::Hello { .. } => {
                log::trace!("Handling of Hello Messages not yet implemented!");
            }
            SessionBody::Scout { .. } => {
                log::trace!("Handling of Scout Messages not yet implemented!");
            }
            _ => {
                log::trace!(
                    "Unexpected message received in session with peer {}: {:?}",
                    self.pid,
                    message
                );
            }
        }
    }
}
