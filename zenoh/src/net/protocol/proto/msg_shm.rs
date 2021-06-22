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
use std::sync::{Arc, RwLock};
#[cfg(feature = "zero-copy")]
use zenoh_util::core::ZResult;

#[cfg(feature = "zero-copy")]
macro_rules! set_shminfo {
    ($msg:expr, $data_info:expr) => {
        // Set the right data info SHM parameters
        if let Some(di) = $data_info {
            di.is_sliced = false;
            if !di.has_options() {
                *$data_info = None;
            }
        }
    };
}

#[cfg(feature = "zero-copy")]
macro_rules! unset_shminfo {
    ($msg:expr, $data_info:expr) => {
        match $data_info {
            Some(di) => {
                // Just update the is_shm field. This field can be
                // then used at receiver side to identify that the
                // actual content is stored in shared memory
                di.is_sliced = true;
            }
            None => {
                // Create the DataInfo content
                *$data_info = Some(DataInfo {
                    kind: None,
                    encoding: None,
                    timestamp: None,
                    is_sliced: true,
                    source_id: None,
                    source_sn: None,
                    first_router_id: None,
                    first_router_sn: None,
                });
            }
        }
    };
}

#[cfg(feature = "zero-copy")]
impl ZenohMessage {
    pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        let mut res = false;

        if let Some(attachment) = self.attachment.as_mut() {
            res = attachment.buffer.map_to_shmbuf(shmr.clone())?;
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            res = res || payload.map_to_shmbuf(shmr)?;

            if !payload.has_shminfo() {
                set_shminfo!(self, data_info);
            }
        }

        Ok(res)
    }

    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let mut res = false;

        if let Some(attachment) = self.attachment.as_mut() {
            res = attachment.buffer.map_to_shminfo()?;
        }

        if let ZenohBody::Data(Data {
            payload, data_info, ..
        }) = &mut self.body
        {
            res = res || payload.map_to_shminfo()?;
            if payload.has_shminfo() {
                unset_shminfo!(self, data_info);
            }
        }

        Ok(res)
    }
}

#[allow(dead_code)]
#[cfg(feature = "zero-copy")]
impl SessionMessage {
    pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        let mut res = false;

        if let Some(attachment) = self.attachment.as_mut() {
            res = attachment.buffer.map_to_shmbuf(shmr)?;
        }

        Ok(res)
    }

    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let mut res = false;

        if let Some(attachment) = self.attachment.as_mut() {
            res = attachment.buffer.map_to_shminfo()?;
        }

        Ok(res)
    }
}
