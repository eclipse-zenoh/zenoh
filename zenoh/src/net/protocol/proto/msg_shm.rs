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
    pub(crate) fn inc_ref_shm(&mut self) {
        if let Some(at) = self.attachment.as_mut() {
            at.buffer.inc_ref_shm();
        }

        if let ZenohBody::Data(Data { payload, .. }) = &mut self.body {
            payload.inc_ref_shm();
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
    pub(crate) fn inc_ref_shm(&mut self) {
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
                        m.inc_ref_shm();
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
