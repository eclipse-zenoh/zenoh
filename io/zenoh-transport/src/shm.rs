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
use std::any::TypeId;
use zenoh_buffers::{ZBuf, ZSlice};
use zenoh_protocol::zenoh::*;
use zenoh_result::ZResult;
use zenoh_shm::{
    SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryBufInfoSerialized, SharedMemoryReader,
};

macro_rules! unset_sliced {
    ($msg:expr, $data_info:expr) => {
        // Set the right data info SHM parameters
        if let Some(di) = $data_info {
            di.sliced = false;
            if di == &DataInfo::default() {
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
                let di = DataInfo {
                    sliced: true,
                    ..Default::default()
                };
                *$data_info = Some(di);
            }
        }
    };
}

pub fn map_zslice_to_shmbuf(zslice: &mut ZSlice, shmr: &mut SharedMemoryReader) -> ZResult<bool> {
    let mut res = false;

    let ZSlice { buf, .. } = zslice;
    if buf.as_any().type_id() == TypeId::of::<SharedMemoryBufInfoSerialized>() {
        // Deserialize the shmb info into shm buff
        let shmbinfo = SharedMemoryBufInfo::deserialize(buf.as_slice())?;

        // First, try in read mode allowing concurrenct lookups
        let smb = shmr
            .try_read_shmbuf(&shmbinfo)
            .or_else(|_| shmr.read_shmbuf(&shmbinfo))?;

        // Replace the content of the slice
        let zs: ZSlice = smb.into();
        *zslice = zs;

        res = true;
    }
    Ok(res)
}

pub fn map_zbuf_to_shmbuf(zbuf: &mut ZBuf, shmr: &mut SharedMemoryReader) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        res |= map_zslice_to_shmbuf(zs, shmr)?;
    }
    Ok(res)
}

pub fn map_zslice_to_shminfo(zslice: &mut ZSlice) -> ZResult<bool> {
    let mut res = false;
    let ZSlice { buf, .. } = zslice;
    if let Some(shmb) = buf.as_any().downcast_ref::<SharedMemoryBuf>() {
        // Serialize the shmb info
        let info: SharedMemoryBufInfoSerialized = shmb.info.serialize()?.into();
        // Increase the reference count so to keep the SharedMemoryBuf valid
        shmb.inc_ref_count();
        // Replace the content of the slice
        let zs: ZSlice = info.into();
        *zslice = zs;

        res = true;
    }
    Ok(res)
}

pub fn map_zbuf_to_shminfo(zbuf: &mut ZBuf) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        res |= map_zslice_to_shminfo(zs)?;
    }
    Ok(res)
}

pub fn map_zmsg_to_shmbuf(msg: &mut ZenohMessage, shmr: &mut SharedMemoryReader) -> ZResult<bool> {
    let mut res = false;

    if let ZenohBody::Data(Data {
        payload, data_info, ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shmbuf(payload, shmr)?;
        unset_sliced!(msg, data_info);
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shmbuf(&mut body.payload, shmr)?;
        body.data_info.sliced = false;
    }

    Ok(res)
}

pub fn map_zmsg_to_shminfo(msg: &mut ZenohMessage) -> ZResult<bool> {
    let mut res = false;

    if let ZenohBody::Data(Data {
        payload, data_info, ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shminfo(payload)?;
        set_sliced!(msg, data_info);
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shminfo(&mut body.payload)?;
        body.data_info.sliced = true;
    }

    Ok(res)
}

// Unused for the time being
//
// #[cfg(feature = "shared-memory")]
// impl TransportMessage {
//     pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
//         Ok(false)
//     }
//
//     pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
//         Ok(false)
//     }
// }
