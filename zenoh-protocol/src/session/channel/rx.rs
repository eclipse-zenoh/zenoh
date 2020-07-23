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
use async_trait::async_trait;

use super::{Channel, DefragBuffer};

use crate::core::{PeerId, ZInt};
use crate::link::Link;
use crate::proto::{FramePayload, SessionBody, SessionMessage, SeqNum, Frame, KeepAlive, Close};
use crate::session::{Action, TransportTrait};

use zenoh_util::{zasynclock, zasyncread, zasyncopt};



/*************************************/
/*         CHANNEL RX STRUCT         */
/*************************************/

pub(super) struct ChannelRxReliable {    
    sn: SeqNum,
    defrag_buffer: DefragBuffer
}

impl ChannelRxReliable {
    pub(super) fn new(        
        initial_sn: ZInt,
        sn_resolution: ZInt
    ) -> ChannelRxReliable {
        // Set the sequence number in the state as it had 
        // received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        ChannelRxReliable {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag_buffer: DefragBuffer::new(initial_sn, sn_resolution)
        }
    }
}

pub(super) struct ChannelRxBestEffort {    
    sn: SeqNum,
    defrag_buffer: DefragBuffer
}

impl ChannelRxBestEffort {
    pub(super) fn new(        
        initial_sn: ZInt,
        sn_resolution: ZInt
    ) -> ChannelRxBestEffort {
        // Set the sequence number in the state as it had 
        // received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        ChannelRxBestEffort {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag_buffer: DefragBuffer::new(initial_sn, sn_resolution)
        }
    }
}

impl Channel {
    /*************************************/
    /*   MESSAGE RECEIVED FROM THE LINK  */
    /*************************************/
    async fn process_reliable_frame(&self, sn: ZInt, payload: FramePayload) -> Action {
        // @TODO: Implement the reordering and reliability. Wait for missing messages.
        let mut guard = zasynclock!(self.rx_reliable);

        match guard.sn.precedes(sn) {
            Ok(precedes) => if precedes {
                // Set will always return OK because we have already checked
                // with precedes() that the sn has the right resolution
                let _ = guard.sn.set(sn);
                match payload {
                    FramePayload::Fragment { buffer, is_final } => {
                        if guard.defrag_buffer.is_empty() {
                            let _ = guard.defrag_buffer.sync(sn);
                        } 
                        let res = guard.defrag_buffer.push(sn, buffer);
                        if res.is_ok() && is_final {
                            match guard.defrag_buffer.defragment() {
                                Ok(msg) => {
                                    log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                                    let _ = zasyncopt!(self.callback).handle_message(msg).await;
                                },
                                Err(e) => log::trace!("Session: {}. Defragmentation error: {:?}", self.get_peer(), e)
                            }
                        }
                    },
                    FramePayload::Messages { mut messages } => {
                        for msg in messages.drain(..) {
                            log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                            let _ = zasyncopt!(self.callback).handle_message(msg).await;
                        }                        
                    }
                }
                // Keep reading
                Action::Read
            } else {
                log::warn!("Reliableframe with invalid SN dropped: {}", sn);
                // Drop the fragments if needed
                if !guard.defrag_buffer.is_empty() {
                    guard.defrag_buffer.clear();
                }
                // Keep reading
                Action::Read
            },
            Err(e) => {
                log::warn!("Invalid SN in reliable frame: {}. \
                            Closing the session with peer: {}", e, self.get_peer());
                // Drop the guard before closing the session
                drop(guard);
                // Delete the whole session
                self.delete().await;
                // Close the link
                Action::Close
            }
        }       
    }

    async fn process_best_effort_frame(&self, sn: ZInt, payload: FramePayload) -> Action {      
        let mut guard = zasynclock!(self.rx_best_effort);

        match guard.sn.precedes(sn) {
            Ok(precedes) => if precedes {
                // Set will always return OK because we have already checked
                // with precedes() that the sn has the right resolution
                let _ = guard.sn.set(sn);
                match payload {
                    FramePayload::Fragment { buffer, is_final } => {
                        if guard.defrag_buffer.is_empty() {
                            let _ = guard.defrag_buffer.sync(sn);
                        } 
                        let res = guard.defrag_buffer.push(sn, buffer);
                        if res.is_ok() && is_final {
                            match guard.defrag_buffer.defragment() {
                                Ok(msg) => {
                                    log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                                    let _ = zasyncopt!(self.callback).handle_message(msg).await;
                                },
                                Err(e) => log::trace!("Session: {}. Defragmentation error: {:?}", self.get_peer(), e)
                            }
                        }
                    },
                    FramePayload::Messages { mut messages } => {
                        for msg in messages.drain(..) {
                            log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                            let _ = zasyncopt!(self.callback).handle_message(msg).await;
                        }                        
                    }
                }
                // Keep reading
                Action::Read
            } else {
                log::warn!("Best effort frame with invalid SN dropped: {}", sn);
                // Drop the fragments if needed
                if !guard.defrag_buffer.is_empty() {
                    guard.defrag_buffer.clear();
                }
                // Keep reading
                Action::Read
            },
            Err(e) => {
                log::warn!("Invalid SN in best effort frame: {}. \
                            Closing the session with peer: {}", e, self.get_peer());
                // Drop the guard before closing the session
                drop(guard);
                // Delete the whole session
                self.delete().await;
                // Close the link
                Action::Close
            }
        }
    }

    async fn process_close(&self, link: &Link, pid: Option<PeerId>, reason: u8, link_only: bool) -> Action {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::warn!("Received an invalid Close on link {} from peer {} with reason: {}. Ignoring.", link, pid, reason);
                return Action::Read
            }
        }        
        
        if link_only {
            // Delete only the link but keep the session open
            let _ = self.del_link(link).await;
        } else { 
            // Close the whole session 
            self.delete().await;
        }
        
        Action::Close
    }

    async fn process_keep_alive(&self, link: &Link, pid: Option<PeerId>) -> Action {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::warn!("Received an invalid KeepAlive on link {} from peer: {}. Ignoring.", link, pid);
                return Action::Read
            }
        }

        Action::Read
    }
}

#[async_trait]
impl TransportTrait for Channel {
    async fn receive_message(&self, link: &Link, message: SessionMessage) -> Action {
        log::trace!("Received from peer {} on link {}: {:?}", self.get_peer(), link, message);

        // Mark the link as alive for link and session lease
        let guard = zasyncread!(self.links);
        if let Some(link) = zlinkget!(guard, link) {
            link.mark_alive();
        } else {
            return Action::Close
        }
        drop(guard);

        // Process the received message
        match message.body {
            SessionBody::Frame(Frame { ch, sn, payload }) => {
                match ch {
                    true => self.process_reliable_frame(sn, payload).await,
                    false => self.process_best_effort_frame(sn, payload).await
                }
            },
            SessionBody::AckNack { .. } => {
                log::debug!("Handling of AckNack Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },
            SessionBody::Close(Close { pid, reason, link_only }) => {
                self.process_close(link, pid, reason, link_only).await
            },
            SessionBody::Hello { .. } => {
                log::debug!("Handling of Hello Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },
            SessionBody::KeepAlive(KeepAlive { pid }) => {
                self.process_keep_alive(link, pid).await
            },            
            SessionBody::Ping { .. } => {
                log::debug!("Handling of Ping Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },
            SessionBody::Pong { .. } => {
                log::debug!("Handling of Pong Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },
            SessionBody::Scout { .. } => {
                log::debug!("Handling of Scout Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },
            SessionBody::Sync { .. } => {
                log::debug!("Handling of Sync Messages not yet implemented!");
                self.delete().await;
                Action::Close
            },            
            SessionBody::Open { .. } |
            SessionBody::Accept { .. } => {
                log::debug!("Unexpected Open/Accept message received in an already established session\
                             Closing the link: {}", link);
                self.delete().await;
                Action::Close
            }
        }        
    }

    async fn link_err(&self, link: &Link) {
        log::warn!("Unexpected error on link {} with peer: {}", link, self.get_peer());
        let _ = self.del_link(link).await;
        let _ = link.close().await;
    }
}
