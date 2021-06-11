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
#[cfg(feature = "zero-copy")]
use super::io::SharedMemoryReader;
use super::msg::*;
#[cfg(feature = "zero-copy")]
use zenoh_util::core::ZResult;

impl ZenohMessage {
    #[cfg(feature = "zero-copy")]
    pub(crate) fn map_to_shmbuf(&mut self, shmm: &mut SharedMemoryReader) -> ZResult<bool> {
        let mut res = false;

        if let Some(at) = self.attachment.as_mut() {
            res |= at.buffer.map_to_shmbuf(shmm)?;
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            let is_shm = payload.map_to_shmbuf(shmm)?;

            if is_shm {
                // Set the right data info SHM parameters
                if let Some(di) = data_info {
                    di.is_shm = false;
                    if !di.has_opts() {
                        *data_info = None;
                        // Unset the DataInfo flag in the header
                        self.header &= !zmsg::flag::I;
                    }
                }
            }

            res |= is_shm;
        }

        Ok(res)
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn try_map_to_shmbuf(&mut self, shmm: &SharedMemoryReader) -> ZResult<bool> {
        let mut res = false;

        if let Some(at) = self.attachment.as_mut() {
            res |= at.buffer.try_map_to_shmbuf(shmm)?;
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            let is_shm = payload.try_map_to_shmbuf(shmm)?;

            if is_shm {
                // Set the right data info SHM parameters
                if let Some(di) = data_info {
                    di.is_shm = false;
                    if !di.has_opts() {
                        *data_info = None;
                        // Unset the DataInfo flag in the header
                        self.header &= !zmsg::flag::I;
                    }
                }
            }

            res |= is_shm;
        }

        Ok(res)
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let mut res = false;

        if let Some(at) = self.attachment.as_mut() {
            res |= at.buffer.map_to_shminfo()?;
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            let is_shm = payload.map_to_shminfo()?;

            if is_shm {
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

            res |= is_shm;
        }

        Ok(res)
    }
}

// These functions are not used at the moment but it might be used in the future.
// It's a good practice to keep the message definitions aligned.
// impl SessionMessage {
//     #[cfg(feature = "zero-copy")]
//     pub(crate) fn flatten_shm(&mut self) {
//         if let Some(at) = self.attachment.as_mut() {
//             at.buffer.flatten_shm();
//         }

//         match &mut self.body {
//             SessionBody::Frame(Frame { payload, .. }) => match payload {
//                 FramePayload::Fragment { buffer, .. } => {
//                     buffer.flatten_shm();
//                 }
//                 FramePayload::Messages { messages } => {
//                     for m in messages {
//                         m.flatten_shm();
//                     }
//                 }
//             },
//             SessionBody::InitAck(InitAck { cookie, .. }) => {
//                 cookie.flatten_shm();
//             }
//             SessionBody::OpenSyn(OpenSyn { cookie, .. }) => {
//                 cookie.flatten_shm();
//             }
//             _ => {}
//         }
//     }

//     #[cfg(feature = "zero-copy")]
//     pub(crate) fn prepare_shm(&mut self) {
//         if let Some(at) = self.attachment.as_mut() {
//             at.buffer.inc_ref_shm();
//         }

//         match &mut self.body {
//             SessionBody::Frame(Frame { payload, .. }) => match payload {
//                 FramePayload::Fragment { buffer, .. } => {
//                     buffer.inc_ref_shm();
//                 }
//                 FramePayload::Messages { messages } => {
//                     for m in messages {
//                         m.prepare_shm();
//                     }
//                 }
//             },
//             SessionBody::InitAck(InitAck { cookie, .. }) => {
//                 cookie.inc_ref_shm();
//             }
//             SessionBody::OpenSyn(OpenSyn { cookie, .. }) => {
//                 cookie.inc_ref_shm();
//             }
//             _ => {}
//         }
//     }
// }
