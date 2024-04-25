// Copyright (c) 2024 ZettaScale Technology
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

use tokio::sync::RwLock;
use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZBuf, ZSlice, ZSliceKind};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{zasyncread, zasyncwrite, zerror};
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push, Request, Response},
    zenoh::{
        err::{ext::ErrBodyType, Err},
        ext::ShmType,
        query::{ext::QueryBodyType, Query},
        PushBody, Put, Reply, RequestBody, ResponseBody,
    },
};
use zenoh_result::ZResult;
use zenoh_shm::{SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryReader};

// Traits
trait MapShm {
    fn map_to_shminfo(&mut self) -> ZResult<bool>;
    fn map_to_shmbuf(&mut self, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool>;
}

macro_rules! map_to_shminfo {
    ($zbuf:expr, $ext_shm:expr) => {{
        let res = map_zbuf_to_shminfo($zbuf)?;
        if res {
            *$ext_shm = Some(ShmType::new());
        }
        Ok(res)
    }};
}

macro_rules! map_to_shmbuf {
    ($zbuf:expr, $ext_shm:expr, $shmr:expr) => {{
        if $ext_shm.is_some() {
            *$ext_shm = None;
            map_zbuf_to_shmbuf($zbuf, $shmr)
        } else {
            Ok(false)
        }
    }};
}

// Impl - Put
impl MapShm for Put {
    fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shminfo!(payload, ext_shm)
    }

    fn map_to_shmbuf(&mut self, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf!(payload, ext_shm, shmr)
    }
}

// Impl - Query
impl MapShm for Query {
    fn map_to_shminfo(&mut self) -> ZResult<bool> {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_shminfo!(payload, ext_shm)
        } else {
            Ok(false)
        }
    }

    fn map_to_shmbuf(&mut self, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_shmbuf!(payload, ext_shm, shmr)
        } else {
            Ok(false)
        }
    }
}

// Impl - Reply
impl MapShm for Reply {
    fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shminfo!(payload, ext_shm)
    }

    fn map_to_shmbuf(&mut self, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf!(payload, ext_shm, shmr)
    }
}

// Impl - Err
impl MapShm for Err {
    fn map_to_shminfo(&mut self) -> ZResult<bool> {
        if let Self {
            ext_body: Some(ErrBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_shminfo!(payload, ext_shm)
        } else {
            Ok(false)
        }
    }

    fn map_to_shmbuf(&mut self, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
        if let Self {
            ext_body: Some(ErrBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_shmbuf!(payload, ext_shm, shmr)
        } else {
            Ok(false)
        }
    }
}

// ShmBuf -> ShmInfo
pub fn map_zmsg_to_shminfo(msg: &mut NetworkMessage) -> ZResult<bool> {
    match &mut msg.body {
        NetworkBody::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_shminfo(),
            PushBody::Del(_) => Ok(false),
        },
        NetworkBody::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_shminfo(),
            RequestBody::Put(b) => b.map_to_shminfo(),
            RequestBody::Del(_) | RequestBody::Pull(_) => Ok(false),
        },
        NetworkBody::Response(Response { payload, .. }) => match payload {
            ResponseBody::Reply(b) => b.map_to_shminfo(),
            ResponseBody::Put(b) => b.map_to_shminfo(),
            ResponseBody::Err(b) => b.map_to_shminfo(),
            ResponseBody::Ack(_) => Ok(false),
        },
        NetworkBody::ResponseFinal(_) | NetworkBody::Declare(_) | NetworkBody::OAM(_) => Ok(false),
    }
}

// Mapping
pub fn map_zbuf_to_shminfo(zbuf: &mut ZBuf) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        if let Some(shmb) = zs.downcast_ref::<SharedMemoryBuf>() {
            *zs = map_zslice_to_shminfo(shmb)?;
            res = true;
        }
    }
    Ok(res)
}

#[cold]
#[inline(never)]
pub fn map_zslice_to_shminfo(shmb: &SharedMemoryBuf) -> ZResult<ZSlice> {
    // Serialize the shmb info
    let codec = Zenoh080::new();
    let mut info = vec![];
    let mut writer = info.writer();
    codec
        .write(&mut writer, &shmb.info)
        .map_err(|e| zerror!("{:?}", e))?;
    // Increase the reference count so to keep the SharedMemoryBuf valid
    shmb.inc_ref_count();
    // Replace the content of the slice
    let mut zslice: ZSlice = info.into();
    zslice.kind = ZSliceKind::ShmPtr;
    Ok(zslice)
}

// ShmInfo -> ShmBuf
pub fn map_zmsg_to_shmbuf(
    msg: &mut NetworkMessage,
    shmr: &RwLock<SharedMemoryReader>,
) -> ZResult<bool> {
    match &mut msg.body {
        NetworkBody::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_shmbuf(shmr),
            PushBody::Del(_) => Ok(false),
        },
        NetworkBody::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_shmbuf(shmr),
            RequestBody::Put(b) => b.map_to_shmbuf(shmr),
            RequestBody::Del(_) | RequestBody::Pull(_) => Ok(false),
        },
        NetworkBody::Response(Response { payload, .. }) => match payload {
            ResponseBody::Put(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Err(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Reply(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Ack(_) => Ok(false),
        },
        NetworkBody::ResponseFinal(_) | NetworkBody::Declare(_) | NetworkBody::OAM(_) => Ok(false),
    }
}

// Mapping
pub fn map_zbuf_to_shmbuf(zbuf: &mut ZBuf, shmr: &RwLock<SharedMemoryReader>) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut().filter(|x| x.kind == ZSliceKind::ShmPtr) {
        res |= map_zslice_to_shmbuf(zs, shmr)?;
    }
    Ok(res)
}

#[cold]
#[inline(never)]
pub fn map_zslice_to_shmbuf(
    zslice: &mut ZSlice,
    shmr: &RwLock<SharedMemoryReader>,
) -> ZResult<bool> {
    // Deserialize the shmb info into shm buff
    let codec = Zenoh080::new();
    let mut reader = zslice.reader();

    let shmbinfo: SharedMemoryBufInfo = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;

    // First, try in read mode allowing concurrenct lookups
    let r_guard = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async { zasyncread!(shmr) })
    });
    let smb = r_guard.try_read_shmbuf(&shmbinfo).or_else(|_| {
        drop(r_guard);
        let mut w_guard = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { zasyncwrite!(shmr) })
        });
        w_guard.read_shmbuf(&shmbinfo)
    })?;

    // Replace the content of the slice
    let zs: ZSlice = smb.into();
    *zslice = zs;

    Ok(true)
}
