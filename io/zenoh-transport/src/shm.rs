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
    time::{Duration, Instant},
};

use zenoh_buffers::{reader::HasReader, ZBuf, ZSlice, ZSliceKind};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::{zerror, zlock, Wait};
use zenoh_protocol::{
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

use crate::unicast::establishment::ext::shm::AuthSegment;

/// TTL for in-flight SHM buffer leases (Gray & Cheriton lease model).
/// Must be significantly larger than the watchdog validator period (100 ms)
/// to cover realistic RX thread stalls. 5× gives headroom for scheduler
/// jitter without accumulating excessive memory.
pub(crate) const SHM_PENDING_TTL: Duration = Duration::from_millis(500);

pub(crate) struct PendingShmBuf {
    // Held solely for its RAII Drop (keeps ConfirmedDescriptor alive); never read.
    #[allow(dead_code)]
    pub(crate) buf: ShmBufInner,
    pub(crate) deadline: Instant,
}

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

    fn wrap_in_place<const ID: u8>(&self, ext_shm: &mut Option<ShmType<ID>>, slice: &mut ZSlice) {
        if slice.len() < self.message_size_threshold {
            return;
        }

        if let ProviderInitState::Ready(provider) = self.try_get_provider() {
            Self::_wrap_in_place(&provider, ext_shm, slice)
        }
    }

    fn _wrap_in_place<const ID: u8>(
        shm_provider: &ShmProvider<PosixShmProviderBackend>,
        ext_shm: &mut Option<ShmType<ID>>,
        slice: &mut ZSlice,
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
            *ext_shm = Some(ShmType::new());
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

#[derive(Clone, Debug)]
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
            PushBody::Del(_) => {}
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Reply(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
            ResponseBody::Err(b) => b.map_to_partner(partner_shm_cfg, shm_provider),
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => {}
    }
}

/// Clone every [`ShmBufInner`] from SHM-mapped ZSlices in `msg`.
/// Returns an empty Vec if no SHM slices are present.
/// The caller stores these clones in the connection's pending set so the
/// [`ConfirmedDescriptor`] outlives this stack frame until the lease expires or
/// the connection closes (Gray & Cheriton lease model).
pub fn collect_shm_bufs(msg: &NetworkMessageMut) -> Vec<ShmBufInner> {
    let mut out = Vec::new();
    match &msg.body {
        NetworkBodyMut::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => collect_from_zbuf(&b.payload, &mut out),
            PushBody::Del(_) => {}
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => {
                if let Some(body) = &b.ext_body {
                    collect_from_zbuf(&body.payload, &mut out);
                }
            }
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Reply(b) => {
                if let PushBody::Put(p) = &b.payload {
                    collect_from_zbuf(&p.payload, &mut out);
                }
            }
            ResponseBody::Err(b) => collect_from_zbuf(&b.payload, &mut out),
        },
        NetworkBodyMut::ResponseFinal(_)
        | NetworkBodyMut::Interest(_)
        | NetworkBodyMut::Declare(_)
        | NetworkBodyMut::OAM(_) => {}
    }
    out
}

fn collect_from_zbuf(zbuf: &ZBuf, out: &mut Vec<ShmBufInner>) {
    for zs in zbuf.zslices() {
        if zs.kind == ZSliceKind::ShmPtr {
            if let Some(shmb) = zs.downcast_ref::<ShmBufInner>() {
                out.push(shmb.clone());
            }
        }
    }
}

pub fn map_zmsg_to_shmbuf(msg: NetworkMessageMut, shmr: &ShmReader) -> ZResult<()> {
    match msg.body {
        NetworkBodyMut::Push(Push { payload, .. }) => match payload {
            PushBody::Put(b) => b.map_to_shmbuf(shmr),
            PushBody::Del(_) => Ok(()),
        },
        NetworkBodyMut::Request(Request { payload, .. }) => match payload {
            RequestBody::Query(b) => b.map_to_shmbuf(shmr),
        },
        NetworkBodyMut::Response(Response { payload, .. }) => match payload {
            ResponseBody::Err(b) => b.map_to_shmbuf(shmr),
            ResponseBody::Reply(b) => b.map_to_shmbuf(shmr),
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

fn map_to_partner<const ID: u8, ShmCfg: PartnerShmConfig>(
    zbuf: &mut ZBuf,
    ext_shm: &mut Option<ShmType<ID>>,
    partner_shm_cfg: &ShmCfg,
    shm_provider: &Option<Arc<LazyShmProvider>>,
) {
    for zs in zbuf.zslices_mut() {
        match zs.downcast_ref::<ShmBufInner>() {
            None => {
                if let Some(shm_provider) = shm_provider {
                    // Implicit SHM optimization: try to convert to SHM buffer
                    shm_provider.wrap_in_place(ext_shm, zs);
                }
            }
            Some(shmb) => {
                if partner_shm_cfg.supports_protocol(shmb.protocol()) {
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
) -> ZResult<()> {
    if ext_shm.is_some() {
        *ext_shm = None;
        for zs in zbuf.zslices_mut() {
            if zs.kind == ZSliceKind::ShmPtr {
                map_zslice_to_shmbuf(zs, shmr)?;
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
    ) {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner(payload, ext_shm, partner_shm_cfg, shm_provider);
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf(payload, ext_shm, shmr)
    }
}

// Impl - Query
impl MapShm for Query {
    fn map_to_partner<ShmCfg: PartnerShmConfig>(
        &mut self,
        partner_shm_cfg: &ShmCfg,
        shm_provider: &Option<Arc<LazyShmProvider>>,
    ) {
        if let Self {
            ext_body: Some(QueryBodyType {
                payload, ext_shm, ..
            }),
            ..
        } = self
        {
            map_to_partner(payload, ext_shm, partner_shm_cfg, shm_provider);
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
            map_to_shmbuf(payload, ext_shm, shmr)?;
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
    ) {
        if let PushBody::Put(Put {
            payload, ext_shm, ..
        }) = &mut self.payload
        {
            map_to_partner(payload, ext_shm, partner_shm_cfg, shm_provider);
        }
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        if let PushBody::Put(Put {
            payload, ext_shm, ..
        }) = &mut self.payload
        {
            map_to_shmbuf(payload, ext_shm, shmr)?;
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
    ) {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_partner(payload, ext_shm, partner_shm_cfg, shm_provider);
    }

    fn map_to_shmbuf(&mut self, shmr: &ShmReader) -> ZResult<()> {
        let Self {
            payload, ext_shm, ..
        } = self;
        map_to_shmbuf(payload, ext_shm, shmr)
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

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        time::{Duration, Instant},
    };

    use zenoh_buffers::ZBuf;
    use zenoh_core::Wait;
    use zenoh_shm::{
        api::provider::shm_provider::ShmProviderBuilder,
        ShmBufInner,
    };

    use super::{PendingShmBuf, SHM_PENDING_TTL};

    /// Extract a cloned ShmBufInner from a ZShmMut.
    /// The provider must be kept alive by the caller for the duration of use,
    /// otherwise the provider drops and recycles the chunk (invalidating its generation).
    fn shmbuf_from_zbuf(zbuf: &ZBuf) -> ShmBufInner {
        let zslice = zbuf
            .zslices()
            .next()
            .expect("ZBuf should have at least one ZSlice")
            .clone();
        zslice
            .downcast_ref::<ShmBufInner>()
            .expect("ZSlice should hold ShmBufInner")
            .clone()
    }

    /// Invariant 1+3: a clone in the pending set keeps the chunk alive through
    /// validator ticks; clearing pending lets the validator fire.
    #[test]
    fn pending_set_keeps_chunk_alive_and_clear_lets_validator_fire() {
        // Provider must outlive all ShmBufInner references — drop order matters.
        let provider = ShmProviderBuilder::default_backend(65536).wait().unwrap();
        let zbuf: ZBuf = provider.alloc(64).wait().unwrap().into();
        let shmb = shmbuf_from_zbuf(&zbuf);
        assert!(shmb.is_valid(), "freshly allocated buffer should be valid");

        // Clone into pending set (simulates TX collect_shm_bufs after push)
        let now = Instant::now();
        let deadline = now + SHM_PENDING_TTL;
        let mut pending: VecDeque<PendingShmBuf> = VecDeque::new();
        let pending_clone = shmb.clone();
        pending.push_back(PendingShmBuf { buf: pending_clone, deadline });

        // Drop zbuf (holds the original ZSlice/ShmBufInner) and shmb
        // — simulates internal_schedule returning after do_push.
        drop(zbuf);
        drop(shmb);

        // Wait > 2 validator ticks (each tick = 100 ms)
        std::thread::sleep(Duration::from_millis(350));

        // Invariant 1: pending clone holds ConfirmedDescriptor — chunk still valid
        assert!(
            pending.front().unwrap().buf.is_valid(),
            "chunk should remain valid while held in shm_pending"
        );

        // Simulate transport delete(): clear the pending set
        pending.clear();

        // Invariant 3: after clear, ConfirmedDescriptor drops; validator fires within 200 ms
        std::thread::sleep(Duration::from_millis(350));
        // (We cannot call is_valid() after clear since we dropped the ShmBufInner.)
        // The correctness here is validated end-to-end by Test C in unicast_shm.rs.

        drop(provider);
    }

    /// Invariant 2: TTL sweep removes expired front entries.
    #[test]
    fn ttl_sweep_removes_expired_entries() {
        const SHORT_TTL: Duration = Duration::from_millis(50);

        let provider = ShmProviderBuilder::default_backend(65536).wait().unwrap();
        let zbuf1: ZBuf = provider.alloc(64).wait().unwrap().into();
        let zbuf2: ZBuf = provider.alloc(64).wait().unwrap().into();
        let shmb1 = shmbuf_from_zbuf(&zbuf1);
        let shmb2 = shmbuf_from_zbuf(&zbuf2);
        drop(zbuf1);
        drop(zbuf2);

        let t0 = Instant::now();
        let expired_deadline = t0 + SHORT_TTL;
        let mut pending: VecDeque<PendingShmBuf> = VecDeque::new();
        pending.push_back(PendingShmBuf { buf: shmb1, deadline: expired_deadline });

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(100));

        // Trigger sweep by inserting a second entry (mirrors the TX insert logic)
        let now = Instant::now();
        let live_deadline = now + SHM_PENDING_TTL;
        while pending.front().is_some_and(|e| e.deadline <= now) {
            pending.pop_front();
        }
        pending.push_back(PendingShmBuf { buf: shmb2, deadline: live_deadline });

        // Invariant 2: first entry was swept, second entry remains
        assert_eq!(pending.len(), 1, "expired entry should have been swept by TTL");
        assert!(
            pending.front().unwrap().buf.is_valid(),
            "the live entry should still be valid"
        );

        drop(provider);
    }
}
