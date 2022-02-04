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
#[cfg(feature = "shared-memory")]
use super::io::SharedMemoryReader;
#[cfg(feature = "shared-memory")]
use super::transport::*;
#[cfg(feature = "shared-memory")]
use super::zenoh::*;
#[cfg(feature = "shared-memory")]
use super::Options;
#[cfg(feature = "shared-memory")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "shared-memory")]
use zenoh_util::core::Result as ZResult;

#[cfg(feature = "shared-memory")]
macro_rules! unset_sliced {
    ($msg:expr, $data_info:expr) => {
        // Set the right data info SHM parameters
        if let Some(di) = $data_info {
            di.sliced = false;
            if !di.has_options() {
                *$data_info = None;
            }
        }
    };
}

#[cfg(feature = "shared-memory")]
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
                let mut di = DataInfo::new();
                di.sliced = true;
                *$data_info = Some(di);
            }
        }
    };
}

#[cfg(feature = "shared-memory")]
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
            if payload.has_shminfo() {
                res = res || payload.map_to_shmbuf(shmr)?;
                unset_sliced!(self, data_info);
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
            if payload.has_shmbuf() {
                res = res || payload.map_to_shminfo()?;
                set_sliced!(self, data_info);
            }
        }

        Ok(res)
    }
}

#[allow(dead_code)]
#[cfg(feature = "shared-memory")]
impl TransportMessage {
    pub(crate) fn map_to_shmbuf(
        &mut self,
        _shmr: Arc<RwLock<SharedMemoryReader>>,
    ) -> ZResult<bool> {
        Ok(false)
    }

    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        Ok(false)
    }
}
