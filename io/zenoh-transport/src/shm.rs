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
use std::{
    collections::HashSet,
    fmt::Debug,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use zenoh_buffers::{reader::HasReader, ZBuf, ZSlice, ZSliceKind};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::{zerror, zlock, Wait};
use zenoh_protocol::{
    network::{NetworkBodyMut, NetworkMessageMut, Push, Request, Response},
    zenoh::{err::Err, ext::ShmType, query::Query, Del, PushBody, Put, RequestBody, ResponseBody},
};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;
use zenoh_shm::{
    api::{
        common::types::ProtocolID,
        protocol_implementations::posix::posix_shm_provider_backend::PosixShmProviderBackend,
        provider::shm_provider::{
            ConstBool, Defragment, GarbageCollect, JustAlloc, ShmProvider, ShmProviderBuilder,
        },
    },
    reader::ShmReader,
    ShmBufInfo, ShmBufInner,
};

use crate::unicast::establishment::ext::shm::AuthSegment;

struct ProviderInitCfg {
    shm_size: NonZeroUsize,
}

pub enum ProviderInitState {
    Initializing,
    Ready(Arc<ShmProvider<PosixShmProviderBackend>>),
    Error,
}

enum ProviderInitStateInner {
    Enabled(ProviderInitCfg),
    Initializing(flume::Receiver<Option<Arc<ShmProvider<PosixShmProviderBackend>>>>),
    Ready(Arc<ShmProvider<PosixShmProviderBackend>>),
    Error,
}

pub struct LazyShmProvider {
    message_size_threshold: usize,
    state: Mutex<ProviderInitStateInner>,
}

impl LazyShmProvider {
    pub fn new(shm_size: NonZeroUsize, message_size_threshold: usize) -> Self {
        let cfg = ProviderInitCfg { shm_size };
        let state = Mutex::new(ProviderInitStateInner::Enabled(cfg));
        Self {
            message_size_threshold,
            state,
        }
    }

    pub fn try_get_provider(&self) -> ProviderInitState {
        let mut lock = zlock!(self.state);
        match &mut *lock {
            ProviderInitStateInner::Enabled(cfg) => {
                let shm_size = cfg.shm_size.get();
                let (sender, receiver) = flume::bounded(1);

                ZRuntime::Application.spawn_blocking(move || {
                    // todo: this is supported since 1.76
                    // let _ = sender.send(
                    //     ShmProviderBuilder::default_backend(shm_size)
                    //         .wait()
                    //         .inspect_err(|err| {
                    //             tracing::error!("Error creating lazy ShmProvider: {err}")
                    //         })
                    //         .map(Arc::new)
                    //         .ok(),
                    // );
                    let _ =
                        sender.send(match ShmProviderBuilder::default_backend(shm_size).wait() {
                            Ok(backend) => Some(Arc::new(backend)),
                            Result::Err(err) => {
                                tracing::error!("Error creating lazy ShmProvider: {err}");
                                None
                            }
                        });
                });

                *lock = ProviderInitStateInner::Initializing(receiver);
                ProviderInitState::Initializing
            }
            ProviderInitStateInner::Initializing(join_handle) => match join_handle.try_recv() {
                Ok(Some(shm_provider)) => {
                    *lock = ProviderInitStateInner::Ready(shm_provider.clone());
                    ProviderInitState::Ready(shm_provider)
                }
                Ok(None) => {
                    *lock = ProviderInitStateInner::Error;
                    ProviderInitState::Error
                }
                Err(_) => ProviderInitState::Initializing,
            },
            ProviderInitStateInner::Ready(shm_provider) => {
                ProviderInitState::Ready(shm_provider.clone())
            }
            ProviderInitStateInner::Error => ProviderInitState::Error,
        }
    }

    fn wrap_in_place<const ID: u8>(&self, slice: &mut ZSlice, shm_ext: &mut Option<ShmType<ID>>) {
        if slice.len() < self.message_size_threshold {
            return;
        }

        if let ProviderInitState::Ready(provider) = self.try_get_provider() {
            Self::_wrap_in_place(&provider, slice, shm_ext)
        }
    }

    fn _wrap_in_place<const ID: u8>(
        shm_provider: &ShmProvider<PosixShmProviderBackend>,
        slice: &mut ZSlice,
        shm_ext: &mut Option<ShmType<ID>>,
    ) {
        if let Ok(mut shmbuf) = unsafe {
            shm_provider
            .alloc(slice.len())
            .with_unsafe_policy::<Defragment<GarbageCollect<JustAlloc, JustAlloc, ConstBool<false>>>>()
            .wait()
        } {
            shmbuf.as_mut().copy_from_slice(slice);
            *slice = shmbuf.into();
            slice.kind = ZSliceKind::ShmPtr;
            *shm_ext = Some(ShmType::new());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportShmConfig {
    partner_protocols: Box<[ProtocolID]>,
}

impl PartnerShmConfig for TransportShmConfig {
    fn supports_protocol(&self, protocol: ProtocolID) -> bool {
        self.partner_protocols.contains(&protocol)
    }
}

impl TransportShmConfig {
    pub fn new(partner_segment: AuthSegment) -> Self {
        let t: HashSet<ProtocolID> = partner_segment.protocols().iter().cloned().collect();
        Self {
            partner_protocols: t.iter().cloned().collect(),
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
    partner_shm_cfg: &ShmCfg,
    shm_provider: &Option<Arc<LazyShmProvider>>,
) {
    match &mut msg.body {
        NetworkBodyMut::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
            PushBody::Del(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Reply(b) => match &mut b.payload {
                PushBody::Put(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
                PushBody::Del(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
            },
            ResponseBody::Err(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => {}
    }
}

pub fn map_zmsg_to_shmbuf(msg: NetworkMessageMut, shmr: &ShmReader) -> ZResult<()> {
    match msg.body {
        NetworkBodyMut::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_shmbuf(shmr),
            PushBody::Del(d) => d.map_to_shmbuf(shmr),
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_shmbuf(shmr),
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Err(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Reply(b) => match &mut b.payload {
                PushBody::Put(b) => b.map_to_shmbuf(shmr),
                PushBody::Del(b) => b.map_to_shmbuf(shmr),
            },
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => Ok(()),
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
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    );
}

fn map_to_partner<ShmCfg: PartnerShmConfig, const ID: u8>(
    buf: &mut ZBuf,
    partner_shm_cfg: &ShmCfg,
    shm_provider: &Option<Arc<LazyShmProvider>>,
    shm_ext: &mut Option<ShmType<ID>>,
) {
    for zs in buf.zslices_mut() {
        match zs.downcast_ref::<ShmBufInner>() {
            None => {
                if let Some(shm_provider) = shm_provider {
                    // Implicit SHM optimization: try to convert to SHM buffer
                    shm_provider.wrap_in_place(zs, shm_ext);
                }
            }
            Some(shmb) => {
                if partner_shm_cfg.supports_protocol(shmb.protocol()) {
                    zs.kind = ZSliceKind::ShmPtr;
                    *shm_ext = Some(ShmType::new());
                }
            }
        }
    }
}

fn map_to_shmbuf(zbuf: &mut ZBuf, shmr: &ShmReader) -> ZResult<()> {
    for zs in zbuf.zslices_mut() {
        if zs.kind == ZSliceKind::ShmPtr {
            map_zslice_to_shmbuf(zs, shmr)?;
        }
    }
    Ok(())
}
pub trait OptionInspectMutExt<T> {
    fn inspect_mut<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T);

    fn try_inspect_mut<F, E>(&mut self, f: F) -> Result<(), E>
    where
        F: FnOnce(&mut T) -> Result<(), E>;
}

impl<T> OptionInspectMutExt<T> for Option<T> {
    fn inspect_mut<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T),
    {
        if let Some(v) = self.as_mut() {
            f(v);
        }
    }

    fn try_inspect_mut<F, E>(&mut self, f: F) -> Result<(), E>
    where
        F: FnOnce(&mut T) -> Result<(), E>,
    {
        match self.as_mut() {
            Some(v) => f(v),
            None => Ok(()),
        }
    }
}

// Impl - Put
impl MapShm for Put {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    ) {
        let Self {
            payload,
            ext_attachment,
            ext_shm,
            ..
        } = self;

        ext_attachment.inspect_mut(|val| {
            map_to_partner(&mut val.buffer, partner_shm_cfg, shm_provider, ext_shm);
        });

        map_to_partner(payload, partner_shm_cfg, shm_provider, ext_shm);
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            payload,
            ext_attachment,
            ..
        } = self;

        ext_attachment.try_inspect_mut(|val| map_to_shmbuf(&mut val.buffer, shmr))?;
        map_to_shmbuf(payload, shmr)
    }
}

// Impl - Query
impl MapShm for Query {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    ) {
        let Self {
            ext_body,
            ext_attachment,
            ext_shm,
            ..
        } = self;

        ext_attachment.inspect_mut(|val| {
            map_to_partner(&mut val.buffer, partner_shm_cfg, shm_provider, ext_shm);
        });

        ext_body.inspect_mut(|val| {
            map_to_partner(&mut val.payload, partner_shm_cfg, shm_provider, ext_shm);
        });
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            ext_body,
            ext_attachment,
            ..
        } = self;

        ext_attachment.try_inspect_mut(|val| map_to_shmbuf(&mut val.buffer, shmr))?;

        ext_body.try_inspect_mut(|val| map_to_shmbuf(&mut val.payload, shmr))?;

        Ok(())
    }
}

// Impl - Query
impl MapShm for Del {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    ) {
        let Self {
            ext_attachment,
            ext_shm,
            ..
        } = self;

        ext_attachment.inspect_mut(|val| {
            map_to_partner(&mut val.buffer, partner_shm_cfg, shm_provider, ext_shm);
        });
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self { ext_attachment, .. } = self;

        ext_attachment.try_inspect_mut(|val| map_to_shmbuf(&mut val.buffer, shmr))?;

        Ok(())
    }
}

// Impl - Err
impl MapShm for Err {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    ) {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner(payload, partner_shm_cfg, shm_provider, ext_shm);
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self { payload, .. } = self;
        map_to_shmbuf(payload, shmr)
    }
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
