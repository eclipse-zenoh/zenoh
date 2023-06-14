//
// Copyright (c) 2023 ZettaScale Technology
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
use async_std::{sync::RwLock, task};
use std::any::TypeId;
use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZBuf, ZSlice};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{zasyncread, zasyncwrite, zerror};
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

#[cold]
#[inline(never)]
pub fn map_zslice_to_shmbuf(
    zslice: &mut ZSlice,
    shmr: &RwLock<SharedMemoryReader>,
) -> ZResult<bool> {
    let ZSlice { buf, .. } = zslice;
    // Deserialize the shmb info into shm buff
    let codec = Zenoh080::new();
    let mut reader = buf.as_slice().reader();
    let shmbinfo: SharedMemoryBufInfo = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;

    // First, try in read mode allowing concurrenct lookups
    let r_guard = task::block_on(async { zasyncread!(shmr) });
    let smb = r_guard.try_read_shmbuf(&shmbinfo).or_else(|_| {
        drop(r_guard);
        let mut w_guard = task::block_on(async { zasyncwrite!(shmr) });
        w_guard.read_shmbuf(&shmbinfo)
    })?;

    // Replace the content of the slice
    let zs: ZSlice = smb.into();
    *zslice = zs;

    Ok(true)
}

pub fn map_zbuf_to_shmbuf(zbuf: &mut ZBuf, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        let ZSlice { buf, .. } = zs;
        if buf.as_any().type_id() == TypeId::of::<SharedMemoryBufInfoSerialized>() {
            res |= map_zslice_to_shmbuf(zs, shmr)?;
        }
    }
    Ok(res)
}

#[cold]
#[inline(never)]
pub fn map_zslice_to_shminfo(shmb: &SharedMemoryBuf) -> ZResult<ZSlice> {
    // Serialize the shmb info
    let codec = Zenoh080::new();
    let mut bytes = vec![];
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, &shmb.info)
        .map_err(|e| zerror!("{:?}", e))?;
    let info: SharedMemoryBufInfoSerialized = bytes.into();
    // Increase the reference count so to keep the SharedMemoryBuf valid
    shmb.inc_ref_count();
    // Replace the content of the slice
    Ok(info.into())
}

pub fn map_zbuf_to_shminfo(zbuf: &mut ZBuf) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        let ZSlice { buf, .. } = zs;
        if let Some(shmb) = buf.as_any().downcast_ref::<SharedMemoryBuf>() {
            *zs = map_zslice_to_shminfo(shmb)?;
            res = true;
        }
    }
    Ok(res)
}

pub fn map_zmsg_to_shmbuf(
    msg: &mut ZenohMessage,
    shmr: &RwLock<SharedMemoryReader>,
) -> ZResult<bool> {
    let mut res = false;

    if let ZenohBody::Data(Data {
        payload, data_info, ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shmbuf(payload, shmr)?;
        if res {
            unset_sliced!(msg, data_info);
        }
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shmbuf(&mut body.payload, shmr)?;
        if res {
            body.data_info.sliced = false;
        }
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
        if res {
            set_sliced!(msg, data_info);
        }
    } else if let ZenohBody::Query(Query {
        body: Some(body), ..
    }) = &mut msg.body
    {
        res |= map_zbuf_to_shminfo(&mut body.payload)?;
        if res {
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
//         Ok(false)
//     }
//
//     pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
//         Ok(false)
//     }
// }
