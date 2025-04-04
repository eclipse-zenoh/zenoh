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
use std::collections::HashSet;

use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZBuf, ZSlice, ZSliceKind};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::zerror;
use zenoh_protocol::{
    network::{
        NetworkBody, NetworkBodyMut, NetworkMessage, NetworkMessageMut, Push, Request, Response,
    },
    zenoh::{
        err::Err,
        ext::ShmType,
        query::{ext::QueryBodyType, Query},
        PushBody, Put, Reply, RequestBody, ResponseBody,
    },
};
use zenoh_result::ZResult;
use zenoh_shm::{api::common::types::ProtocolID, reader::ShmReader, ShmBufInfo, ShmBufInner};

use crate::unicast::establishment::ext::shm::AuthSegment;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportShmConfig {
    partner_protocols: HashSet<ProtocolID>,
}

impl PartnerShmConfig for TransportShmConfig {
    fn supports_protocol(&self, protocol: ProtocolID) -> bool {
        self.partner_protocols.contains(&protocol)
    }
}

impl TransportShmConfig {
    pub fn new(partner_segment: AuthSegment) -> Self {
        Self {
            partner_protocols: partner_segment.protocols().iter().cloned().collect(),
        }
    }
}

#[derive(Clone)]
pub struct MulticastTransportShmConfig;

impl PartnerShmConfig for MulticastTransportShmConfig {
    fn supports_protocol(&self, _protocol: ProtocolID) -> bool {
        true
    }
}

pub fn map_zmsg_to_partner<ShmCfg: PartnerShmConfig>(
    msg: &mut NetworkMessageMut,
    partner_shm_cfg: &Option<ShmCfg>,
) -> ZResult<()> {
    match &mut msg.body {
        NetworkBodyMut::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_partner(partner_shm_cfg),
            PushBody::Del(_) => Ok(()),
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_partner(partner_shm_cfg),
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Reply(b) => b.map_to_partner(partner_shm_cfg),
            ResponseBody::Err(b) => b.map_to_partner(partner_shm_cfg),
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => Ok(()),
    }
}

pub fn map_zmsg_to_shmbuf(msg: &mut NetworkMessage, shmr: &ShmReader) -> ZResult<()> {
    match &mut msg.body {
        NetworkBody::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_shmbuf(shmr),
            PushBody::Del(_) => Ok(()),
        },
        NetworkBody::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_shmbuf(shmr),
        },
        NetworkBody::Response(Response { payload, .. }) => match payload {
            ResponseBody::Err(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Reply(b) => b.map_to_shmbuf(shmr),
        },
        NetworkBody::ResponseFinal(_)
        | NetworkBody::Interest(_)
        | NetworkBody::Declare(_)
        | NetworkBody::OAM(_) => Ok(()),
    }
}

pub trait PartnerShmConfig {
    fn supports_protocol(&self, protocol: ProtocolID) -> bool;
}

// Currently, there can be three forms of ZSlice:
// rawbuf - usual non-shm buffer
// shminfo - small SHM info that can be used to mount SHM buffer and get access to it's contents
// shmbuf - mounted SHM buffer
// On RX and TX we need to do the following conversion:
trait MapShm {
    // RX:
    // - shminfo -> shmbuf
    // - rawbuf -> rawbuf (no changes)
    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()>;

    // TX:
    // - shmbuf -> shminfo if partner supports shmbuf's SHM protocol
    // - shmbuf -> rawbuf if partner does not support shmbuf's SHM protocol
    // - rawbuf -> rawbuf (no changes)
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &Option<ShmCfg>,
    ) -> ZResult<()>;
}

macro_rules! map_to_partner {
    ($zbuf:expr, $ext_shm:expr, $partner_shm_cfg:expr) => {{
        match $partner_shm_cfg {
            Some(shm_cfg) => {
                let res = to_shm_partner($zbuf, shm_cfg)?;
                if res {
                    *$ext_shm = Some(ShmType::new());
                }
            }
            None => {
                to_non_shm_partner($zbuf);
            }
        }

        Ok(())
    }};
}

macro_rules! map_zbuf_to_shmbuf {
    ($zbuf:expr, $ext_shm:expr, $shmr:expr) => {{
        if $ext_shm.is_some() {
            *$ext_shm = None;
            map_zbuf_to_shmbuf($zbuf, $shmr)
        } else {
            Ok(())
        }
    }};
}

// Impl - Put
impl MapShm for Put {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &Option<ShmCfg>,
    ) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner!(payload, ext_shm, partner_shm_cfg)
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_zbuf_to_shmbuf!(payload, ext_shm, shmr)
    }
}

// Impl - Query
impl MapShm for Query {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &Option<ShmCfg>,
    ) -> ZResult<()> {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_partner!(payload, ext_shm, partner_shm_cfg)
        } else {
            Ok(())
        }
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_zbuf_to_shmbuf!(payload, ext_shm, shmr)
        } else {
            Ok(())
        }
    }
}

// Impl - Reply
impl MapShm for Reply {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &Option<ShmCfg>,
    ) -> ZResult<()> {
        match &mut self.payload {
            PushBody::Put(put) => {
                let Put {
                    payload, ext_shm, ..
                } = put;
                map_to_partner!(payload, ext_shm, partner_shm_cfg)
            }
            PushBody::Del(_) => Ok(()),
        }
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        match &mut self.payload {
            PushBody::Put(put) => {
                let Put {
                    payload, ext_shm, ..
                } = put;
                map_zbuf_to_shmbuf!(payload, ext_shm, shmr)
            }
            PushBody::Del(_) => Ok(()),
        }
    }
}

// Impl - Err
impl MapShm for Err {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &Option<ShmCfg>,
    ) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner!(payload, ext_shm, partner_shm_cfg)
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_zbuf_to_shmbuf!(payload, ext_shm, shmr)
    }
}

#[cold]
#[inline(never)]
pub fn shmbuf_to_rawbuf(shmb: &ShmBufInner) -> ZSlice {
    // Convert shmb to raw buffer
    // TODO: optimize this! We should not make additional buffer copy here,
    // but we need to make serializer serialize SHM buffer as raw buffer.
    shmb.as_ref().to_vec().into()
}

#[cold]
#[inline(never)]
pub fn shmbuf_to_shminfo(shmb: &ShmBufInner) -> ZResult<ZSlice> {
    // Serialize the shmb info
    let codec = Zenoh080::new();
    let mut info = vec![];
    let mut writer = info.writer();
    codec
        .write(&mut writer, &shmb.info)
        .map_err(|e| zerror!("{:?}", e))?;
    // Increase the reference count so to keep the ShmBufInner valid
    unsafe { shmb.inc_ref_count() };
    // Replace the content of the slice
    let mut zslice: ZSlice = info.into();
    zslice.kind = ZSliceKind::ShmPtr;
    Ok(zslice)
}

fn to_shm_partner<ShmCfg: PartnerShmConfig>(
    zbuf: &mut ZBuf,
    partner_shm_cfg: &ShmCfg,
) -> ZResult<bool> {
    let mut res = false;
    for zs in zbuf.zslices_mut() {
        if let Some(shmb) = zs.downcast_ref::<ShmBufInner>() {
            if partner_shm_cfg.supports_protocol(shmb.protocol()) {
                *zs = shmbuf_to_shminfo(shmb)?;
                res = true;
            } else {
                // Replace the content of the slice with rawbuf
                *zs = shmbuf_to_rawbuf(shmb)
            }
        }
    }
    Ok(res)
}

fn to_non_shm_partner(zbuf: &mut ZBuf) {
    for zs in zbuf.zslices_mut() {
        if let Some(shmb) = zs.downcast_ref::<ShmBufInner>() {
            // Replace the content of the slice with rawbuf
            *zs = shmbuf_to_rawbuf(shmb)
        }
    }
}

pub fn map_zbuf_to_shmbuf(zbuf: &mut ZBuf, shmr: &ShmReader) -> ZResult<()> {
    for zs in zbuf.zslices_mut().filter(|x| x.kind == ZSliceKind::ShmPtr) {
        map_zslice_to_shmbuf(zs, shmr)?;
    }
    Ok(())
}

#[cold]
#[inline(never)]
pub fn map_zslice_to_shmbuf(zslice: &mut ZSlice, shmr: &ShmReader) -> ZResult<()> {
    let codec = Zenoh080::new();
    let mut reader = zslice.reader();

    // Deserialize the shminfo
    let shmbinfo: ShmBufInfo = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;

    // Mount shmbuf
    let smb = shmr.read_shmbuf(shmbinfo)?;

    // Replace the content of the slice
    *zslice = smb.into();

    Ok(())
}
