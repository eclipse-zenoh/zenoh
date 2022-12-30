//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use std::sync::{Arc, RwLock};
use zenoh_buffers::SharedMemoryReader;
use zenoh_core::Result as ZResult;
use zenoh_protocol::zenoh::*;

macro_rules! unset_sliced {
    ($msg:expr, $data_info:expr) => {
        // Set the right data info SHM parameters
        if let Some(di) = $data_info {
            di.sliced = false;
            if di != &DataInfo::default() {
                *$data_info = None;
            }
        }
    };
}

macro_rules! set_sliced {
    ($msg:expr, $data_info:expr) => {
        match $data_info {
            Some(di) => {
                // Just update the is_shm field. This field can be
                // then used at receiver side to identify that the
                // actual content is stored in shared memory
                di.sliced = true;
            }
            None => {
                // Create the DataInfo content
                let mut di = DataInfo::default();
                di.sliced = true;
                *$data_info = Some(di);
            }
        }
    };
}

pub fn map_to_shmbuf(
    msg: &mut ZenohMessage,
    shmr: Arc<RwLock<SharedMemoryReader>>,
) -> ZResult<bool> {
    let mut res = false;

    if let Some(attachment) = msg.attachment.as_mut() {
        res = attachment.buffer.map_to_shmbuf(shmr.clone())?;
    }

    if let ZenohBody::Data(Data {
        payload, data_info, ..
    }) = &mut msg.body
    {
        if payload.has_shminfo() {
            res = res || payload.map_to_shmbuf(shmr)?;
            unset_sliced!(msg, data_info);
        }
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        if body.payload.has_shminfo() {
            res = res || body.payload.map_to_shmbuf(shmr)?;
            body.data_info.sliced = false;
        }
    }

    Ok(res)
}

pub fn map_to_shminfo(msg: &mut ZenohMessage) -> ZResult<bool> {
    let mut res = false;

    if let Some(attachment) = msg.attachment.as_mut() {
        res = attachment.buffer.map_to_shminfo()?;
    }

    if let ZenohBody::Data(Data {
        payload, data_info, ..
    }) = &mut msg.body
    {
        if payload.has_shmbuf() {
            res = res || payload.map_to_shminfo()?;
            set_sliced!(msg, data_info);
        }
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        if body.payload.has_shmbuf() {
            res = res || body.payload.map_to_shminfo()?;
            body.data_info.sliced = true;
        }
    }

    Ok(res)
}

// Unused for the time being
//
// #[cfg(feature = "shared-memory")]
// impl TransportMessage {
//     pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
//         let mut res = false;

//         if let Some(attachment) = self.attachment.as_mut() {
//             res = attachment.buffer.map_to_shmbuf(shmr)?;
//         }

//         Ok(res)
//     }

//     pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
//         let mut res = false;

//         if let Some(attachment) = self.attachment.as_mut() {
//             res = attachment.buffer.map_to_shminfo()?;
//         }

//         Ok(res)
//     }
// }
