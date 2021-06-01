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
use super::msg::*;

impl ZenohMessage {
    #[cfg(feature = "zero-copy")]
    pub(crate) fn flatten_shm(&mut self) {
        if let Some(at) = self.attachment.as_mut() {
            at.buffer.flatten_shm();
        }

        if let ZenohBody::Data(Data { payload, .. }) = &mut self.body {
            payload.flatten_shm();
        }
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn prepare_shm(&mut self) {
        if let Some(at) = self.attachment.as_mut() {
            at.buffer.inc_ref_shm();
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            if payload.is_shm() {
                // Increment the reference counter
                payload.inc_ref_shm();

                // Set the right data info SHM parameters
                match data_info {
                    Some(di) => {
                        // Just update the is_shm field. This field can be
                        // then used at receiver side to identify that the
                        // actual content is stored in shared memory
                        di.is_shm = true;
                    }
                    None => {
                        // Create the DataInfo content
                        *data_info = Some(DataInfo {
                            source_id: None,
                            source_sn: None,
                            first_router_id: None,
                            first_router_sn: None,
                            timestamp: None,
                            kind: None,
                            encoding: None,
                            is_shm: true,
                        });
                        // Set the DataInfo flag in the header
                        self.header |= zmsg::flag::I;
                    }
                }
            }
        }
    }
}

impl SessionMessage {
    // This function is not used at the moment but it might be used in the future.
    // It's good to keep the message definitions aligned.
    #[allow(dead_code)]
    #[cfg(feature = "zero-copy")]
    pub(crate) fn flatten_shm(&mut self) {
        if let Some(at) = self.attachment.as_mut() {
            at.buffer.flatten_shm();
        }

        match &mut self.body {
            SessionBody::Frame(Frame { payload, .. }) => match payload {
                FramePayload::Fragment { buffer, .. } => {
                    buffer.flatten_shm();
                }
                FramePayload::Messages { messages } => {
                    for m in messages {
                        m.flatten_shm();
                    }
                }
            },
            SessionBody::InitAck(InitAck { cookie, .. }) => {
                cookie.flatten_shm();
            }
            SessionBody::OpenSyn(OpenSyn { cookie, .. }) => {
                cookie.flatten_shm();
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    #[cfg(feature = "zero-copy")]
    pub(crate) fn prepare_shm(&mut self) {
        if let Some(at) = self.attachment.as_mut() {
            at.buffer.inc_ref_shm();
        }

        match &mut self.body {
            SessionBody::Frame(Frame { payload, .. }) => match payload {
                FramePayload::Fragment { buffer, .. } => {
                    buffer.inc_ref_shm();
                }
                FramePayload::Messages { messages } => {
                    for m in messages {
                        m.prepare_shm();
                    }
                }
            },
            SessionBody::InitAck(InitAck { cookie, .. }) => {
                cookie.inc_ref_shm();
            }
            SessionBody::OpenSyn(OpenSyn { cookie, .. }) => {
                cookie.inc_ref_shm();
            }
            _ => {}
        }
    }
}
