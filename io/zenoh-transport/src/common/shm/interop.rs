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
    fmt::Debug,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use zenoh_buffers::{reader::HasReader, ZBuf, ZSlice, ZSliceKind};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::{zerror, zlock, Wait};
use zenoh_protocol::{
    core::Priority,
    network::{NetworkBodyMut, NetworkMessageMut, Push, Request, Response},
    zenoh::{
        err::Err,
        ext::ShmType,
        query::{ext::QueryBodyType, Query},
        PushBody, Put, Reply, RequestBody, ResponseBody,
    },
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

use crate::unicast::establishment::ext::shm::{
    handoff::{RxHandoffChannel, TxHandoffChannel, TxHandoffStorage, TxHandoffTransaction},
    segment::RXAuthSegment,
};

#[derive(Debug)]
struct ProviderInitCfg {
    shm_size: NonZeroUsize,
}

#[derive(Debug)]
pub enum ProviderInitState {
    Initializing(flume::Receiver<Option<Arc<ShmProvider<PosixShmProviderBackend>>>>),
    Ready(Arc<ShmProvider<PosixShmProviderBackend>>),
    Error,
}

enum ProviderInitStateInner {
    Enabled(ProviderInitCfg),
    Initializing(flume::Receiver<Option<Arc<ShmProvider<PosixShmProviderBackend>>>>),
    Ready(Arc<ShmProvider<PosixShmProviderBackend>>),
    Error,
}

impl std::fmt::Debug for ProviderInitStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled(cfg) => f.debug_tuple("Enabled").field(cfg).finish(),
            Self::Initializing(_) => f.debug_tuple("Initializing").field(&"..").finish(),
            Self::Ready(provider) => f.debug_tuple("Ready").field(provider).finish(),
            Self::Error => f.debug_tuple("Error").finish(),
        }
    }
}

pub struct LazyShmProvider {
    message_size_threshold: usize,
    state: Mutex<ProviderInitStateInner>,
}

impl std::fmt::Debug for LazyShmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyShmProvider")
            .field("message_size_threshold", &self.message_size_threshold)
            .field("state", &self.state)
            .finish()
    }
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

                *lock = ProviderInitStateInner::Initializing(receiver.clone());
                ProviderInitState::Initializing(receiver)
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
                Err(_) => ProviderInitState::Initializing(join_handle.clone()),
            },
            ProviderInitStateInner::Ready(shm_provider) => {
                ProviderInitState::Ready(shm_provider.clone())
            }
            ProviderInitStateInner::Error => ProviderInitState::Error,
        }
    }

    fn wrap_in_place<const ID: u8>(
        &self,
        ext_shm: &mut Option<ShmType<ID>>,
        slice: &mut ZSlice,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) {
        if slice.len() >= self.message_size_threshold {
            if let ProviderInitState::Ready(provider) = self.try_get_provider() {
                Self::_wrap_in_place(&provider, ext_shm, slice, handoff_transaction);
            }
        }
    }

    fn _wrap_in_place<const ID: u8>(
        shm_provider: &ShmProvider<PosixShmProviderBackend>,
        ext_shm: &mut Option<ShmType<ID>>,
        slice: &mut ZSlice,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) -> bool {
        if let Ok(mut shmbuf) = unsafe {
            shm_provider
            .alloc(slice.len())
            .with_unsafe_policy::<Defragment<GarbageCollect<JustAlloc, JustAlloc, ConstBool<false>>>>()
            .wait()
        } {
            shmbuf.as_mut().copy_from_slice(slice);
            let reference = (&shmbuf).into();
            *slice = shmbuf.into();
            slice.kind = ZSliceKind::ShmPtr;
            *ext_shm = Some(ShmType::new());
            handoff_transaction.as_ref().inspect(|transaction| {
                transaction.on_tx(reference);
            });
            return true;
        }
        false
    }
}

#[derive(Debug)]
pub struct LinkShmHandoffConfig {
    pub rx: RxHandoffChannel,
    pub tx: TxHandoffChannel,
}

impl LinkShmHandoffConfig {
    pub fn new(rx: RxHandoffChannel, tx: TxHandoffChannel) -> Self {
        Self { rx, tx }
    }

    pub fn new_disabled() -> Self {
        Self {
            rx: RxHandoffChannel::Disabled,
            tx: TxHandoffChannel::new_disabled(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportShmConfig {
    link_partner_segment: Arc<RXAuthSegment>,
}

impl TransportShmConfig {
    pub fn new(link_partner_segment: Arc<RXAuthSegment>) -> Self {
        Self {
            link_partner_segment,
        }
    }
}

impl PartnerShmConfig for TransportShmConfig {
    fn supports_protocol(&self, protocol: ProtocolID) -> bool {
        self.link_partner_segment.protocols().contains(&protocol)
    }
}

#[derive(Clone, Debug)]
pub struct MulticastTransportShmConfig;

impl PartnerShmConfig for MulticastTransportShmConfig {
    fn supports_protocol(&self, _protocol: ProtocolID) -> bool {
        true
    }
}

pub fn map_zmsg_to_partner<'a, ShmCfg: PartnerShmConfig>(
    msg: &mut NetworkMessageMut,
    partner_shm_cfg: &ShmCfg,
    shm_provider: &Option<Arc<LazyShmProvider>>,
    handoff: &'a TxHandoffStorage,
) -> Option<TxHandoffTransaction> {
    match &mut msg.body {
        NetworkBodyMut::Push(Push {
            payload, ext_qos, ..
        }) => match payload {
            PushBody::Put(b) => {
                let transaction = TxHandoffTransaction::new(handoff, ext_qos.get_priority());
                b.map_to_partner(partner_shm_cfg, shm_provider, &transaction);
                transaction
            }
            PushBody::Del(_) => None,
        },
        NetworkBodyMut::Request(Request {
            payload, ext_qos, ..
        }) => match payload {
            RequestBody::Query(b) => {
                let transaction = TxHandoffTransaction::new(handoff, ext_qos.get_priority());
                b.map_to_partner(partner_shm_cfg, shm_provider, &transaction);
                transaction
            }
        },
        NetworkBodyMut::Response(Response {
            payload, ext_qos, ..
        }) => match payload {
            ResponseBody::Reply(b) => {
                let transaction = TxHandoffTransaction::new(handoff, ext_qos.get_priority());
                b.map_to_partner(partner_shm_cfg, shm_provider, &transaction);
                transaction
            }
            ResponseBody::Err(b) => {
                let transaction = TxHandoffTransaction::new(handoff, ext_qos.get_priority());
                b.map_to_partner(partner_shm_cfg, shm_provider, &transaction);
                transaction
            }
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => None,
    }
}

pub fn map_zmsg_to_shmbuf(
    msg: NetworkMessageMut,
    shmr: &ShmReader,
    handoff: &RxHandoffChannel,
) -> ZResult<()> {
    match msg.body {
        NetworkBodyMut::Push(Push {
            payload, ext_qos, ..
        }) => match payload {
            PushBody::Put(b) => b.map_to_shmbuf(shmr, handoff, ext_qos.get_priority()),
            PushBody::Del(_) => Ok(()),
        },
        NetworkBodyMut::Request(Request {
            payload, ext_qos, ..
        }) => match payload {
            RequestBody::Query(b) => b.map_to_shmbuf(shmr, handoff, ext_qos.get_priority()),
        },
        NetworkBodyMut::Response(Response {
            payload, ext_qos, ..
        }) => match payload {
            ResponseBody::Err(b) => b.map_to_shmbuf(shmr, handoff, ext_qos.get_priority()),
            ResponseBody::Reply(b) => b.map_to_shmbuf(shmr, handoff, ext_qos.get_priority()),
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
    fn map_to_shmbuf(
        &mut self,
        shmr: &ShmReader,
        handoff: &RxHandoffChannel,
        priority: Priority,
    ) -> ZResult<()>;

    // TX:
    // - shmbuf -> shminfo if partner supports shmbuf's SHM protocol
    // - shmbuf -> rawbuf if partner does not support shmbuf's SHM protocol
    // - rawbuf -> rawbuf (no changes)
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
        handoff_transaction: &Option<TxHandoffTransaction>,
    );
}

fn map_to_partner<const ID: u8, ShmCfg: PartnerShmConfig>(
    zbuf: &mut ZBuf,
    ext_shm: &mut Option<ShmType<ID>>,
    partner_shm_cfg: &ShmCfg,
    shm_provider: &Option<Arc<LazyShmProvider>>,
    handoff_transaction: &Option<TxHandoffTransaction>,
) {
    for zs in zbuf.zslices_mut() {
        match zs.downcast_ref::<ShmBufInner>() {
            None => {
                if let Some(shm_provider) = shm_provider {
                    // Implicit SHM optimization: try to convert to SHM buffer
                    shm_provider.wrap_in_place(ext_shm, zs, handoff_transaction);
                }
            }
            Some(shmb) => {
                if partner_shm_cfg.supports_protocol(shmb.protocol()) {
                    handoff_transaction.as_ref().inspect(|transaction| {
                        transaction.on_tx(shmb.into());
                    });
                    zs.kind = ZSliceKind::ShmPtr;
                    *ext_shm = Some(ShmType::new());
                }
            }
        }
    }
}

fn map_to_shmbuf<const ID: u8>(
    zbuf: &mut ZBuf,
    ext_shm: &mut Option<ShmType<ID>>,
    shmr: &ShmReader,
    handoff: &RxHandoffChannel,
    priority: Priority,
) -> ZResult<()> {
    if ext_shm.is_some() {
        *ext_shm = None;
        for zs in zbuf.zslices_mut() {
            if zs.kind == ZSliceKind::ShmPtr {
                map_zslice_to_shmbuf(zs, shmr, handoff, priority)?;
            }
        }
    }
    Ok(())
}

// Impl - Put
impl MapShm for Put {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner(
            payload,
            ext_shm,
            partner_shm_cfg,
            shm_provider,
            handoff_transaction,
        );
    }

    fn map_to_shmbuf(
        &mut self,
        shmr: &ShmReader,
        handoff: &RxHandoffChannel,
        priority: Priority,
    ) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf(payload, ext_shm, shmr, handoff, priority)
    }
}

// Impl - Query
impl MapShm for Query {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_partner(
                payload,
                ext_shm,
                partner_shm_cfg,
                shm_provider,
                handoff_transaction,
            );
        }
    }

    fn map_to_shmbuf(
        &mut self,
        shmr: &ShmReader,
        handoff: &RxHandoffChannel,
        priority: Priority,
    ) -> ZResult<()> {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_shmbuf(payload, ext_shm, shmr, handoff, priority)?;
        }
        Ok(())
    }
}

// Impl - Reply
impl MapShm for Reply {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) {
        if let PushBody::Put(Put {
            payload, ext_shm, ..
        }) = &mut self.payload
        {
            map_to_partner(
                payload,
                ext_shm,
                partner_shm_cfg,
                shm_provider,
                handoff_transaction,
            );
        }
    }

    fn map_to_shmbuf(
        &mut self,
        shmr: &ShmReader,
        handoff: &RxHandoffChannel,
        priority: Priority,
    ) -> ZResult<()> {
        if let PushBody::Put(Put {
            payload, ext_shm, ..
        }) = &mut self.payload
        {
            map_to_shmbuf(payload, ext_shm, shmr, handoff, priority)?;
        }
        Ok(())
    }
}

// Impl - Err
impl MapShm for Err {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
        handoff_transaction: &Option<TxHandoffTransaction>,
    ) {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner(
            payload,
            ext_shm,
            partner_shm_cfg,
            shm_provider,
            handoff_transaction,
        );
    }

    fn map_to_shmbuf(
        &mut self,
        shmr: &ShmReader,
        handoff: &RxHandoffChannel,
        priority: Priority,
    ) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf(payload, ext_shm, shmr, handoff, priority)
    }
}

#[cold]
#[inline(never)]
pub fn map_zslice_to_shmbuf(
    zslice: &mut ZSlice,
    shmr: &ShmReader,
    handoff: &RxHandoffChannel,
    priority: Priority,
) -> ZResult<()> {
    let codec = Zenoh080::new();
    let mut reader = zslice.reader();

    // Deserialize the shminfo
    let shmbinfo: ShmBufInfo = codec.read(&mut reader).map_err(|e| zerror!("{:?}", e))?;

    // Mount shmbuf
    let smb = shmr.read_shmbuf(shmbinfo)?;

    // Handle RX handoff
    handoff.on_rx(priority);

    // Replace the content of the slice
    *zslice = smb.into();

    Ok(())
}
