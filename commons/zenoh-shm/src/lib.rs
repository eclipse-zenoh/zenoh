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
use api::{common::types::ProtocolID, provider::chunk::ChunkDescriptor};
use header::descriptor::{HeaderDescriptor, OwnedHeaderDescriptor};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicPtr, Ordering},
        Arc,
    },
};
use watchdog::{confirmator::ConfirmedDescriptor, descriptor::Descriptor};
use zenoh_buffers::ZSliceBuffer;

#[macro_export]
macro_rules! tested_module {
    ($module:ident) => {
        #[cfg(feature = "test")]
        pub mod $module;
        #[cfg(not(feature = "test"))]
        mod $module;
    };
}

#[macro_export]
macro_rules! tested_crate_module {
    ($module:ident) => {
        #[cfg(feature = "test")]
        pub mod $module;
        #[cfg(not(feature = "test"))]
        pub(crate) mod $module;
    };
}

pub mod api;
pub mod header;
pub mod posix_shm;
pub mod reader;
pub mod watchdog;
pub mod zsliceshm_access;

/// Informations about a [`SharedMemoryBuf`].
///
/// This that can be serialized and can be used to retrieve the [`SharedMemoryBuf`] in a remote process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedMemoryBufInfo {
    /// The data chunk descriptor
    pub data_descriptor: ChunkDescriptor,
    /// Protocol identifier for particular SharedMemory implementation
    pub shm_protocol: ProtocolID,
    /// Actual data length
    /// NOTE: data_descriptor's len is >= of this len and describes the actual memory length
    /// dedicated in shared memory segment for this particular buffer.
    pub data_len: usize,

    /// The watchdog descriptor
    pub watchdog_descriptor: Descriptor,
    /// The header descriptor
    pub header_descriptor: HeaderDescriptor,
    /// The generation of the buffer
    pub generation: u32,
}

impl SharedMemoryBufInfo {
    pub fn new(
        data_descriptor: ChunkDescriptor,
        shm_protocol: ProtocolID,
        data_len: usize,
        watchdog_descriptor: Descriptor,
        header_descriptor: HeaderDescriptor,
        generation: u32,
    ) -> SharedMemoryBufInfo {
        SharedMemoryBufInfo {
            data_descriptor,
            shm_protocol,
            data_len,
            watchdog_descriptor,
            header_descriptor,
            generation,
        }
    }
}

/// A zenoh buffer in shared memory.
#[non_exhaustive]
pub struct SharedMemoryBuf {
    pub(crate) header: OwnedHeaderDescriptor,
    pub(crate) buf: AtomicPtr<u8>,
    pub info: SharedMemoryBufInfo,
    pub(crate) watchdog: Arc<ConfirmedDescriptor>,
}

impl std::fmt::Debug for SharedMemoryBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemoryBuf")
            .field("header", &self.header)
            .field("buf", &self.buf)
            .field("info", &self.info)
            .finish()
    }
}

impl SharedMemoryBuf {
    pub fn len(&self) -> usize {
        self.info.data_len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_valid(&self) -> bool {
        self.header.header().generation.load(Ordering::SeqCst) == self.info.generation
    }

    pub fn is_unique(&self) -> bool {
        self.ref_count() == 1
    }

    pub fn ref_count(&self) -> u32 {
        self.header.header().refcount.load(Ordering::SeqCst)
    }

    /// Increments buffer's reference count
    ///
    /// # Safety
    /// You should understand what you are doing, as overestimation
    /// of the reference counter can lead to memory being stalled until
    /// recovered by watchdog subsystem or forcely deallocated
    pub unsafe fn inc_ref_count(&self) {
        self.header.header().refcount.fetch_add(1, Ordering::SeqCst);
    }

    // PRIVATE:
    fn as_slice(&self) -> &[u8] {
        log::trace!(
            "SharedMemoryBuf::as_slice() == len = {:?}",
            self.info.data_len
        );
        let bp = self.buf.load(Ordering::SeqCst);
        unsafe { std::slice::from_raw_parts(bp, self.info.data_len) }
    }

    unsafe fn dec_ref_count(&self) {
        self.header.header().refcount.fetch_sub(1, Ordering::SeqCst);
    }

    /// Gets a mutable slice.
    ///
    /// # Safety
    /// This operation is marked unsafe since we cannot guarantee the single mutable reference
    /// across multiple processes. Thus if you use it, and you'll inevitable have to use it,
    /// you have to keep in mind that if you have multiple process retrieving a mutable slice
    /// you may get into concurrent writes. That said, if you have a serial pipeline and
    /// the buffer is flowing through the pipeline this will not create any issues.
    ///
    /// In short, whilst this operation is marked as unsafe, you are safe if you can
    /// guarantee that your in applications only one process at the time will actually write.
    unsafe fn as_mut_slice_inner(&mut self) -> &mut [u8] {
        let bp = self.buf.load(Ordering::SeqCst);
        std::slice::from_raw_parts_mut(bp, self.info.data_len)
    }
}

impl Drop for SharedMemoryBuf {
    fn drop(&mut self) {
        // # Safety
        // obviouly, we need to decrement refcount when dropping SharedMemoryBuf instance
        unsafe { self.dec_ref_count() };
    }
}

impl Clone for SharedMemoryBuf {
    fn clone(&self) -> Self {
        // # Safety
        // obviouly, we need to decrement refcount when cloning SharedMemoryBuf instance
        unsafe { self.inc_ref_count() };
        let bp = self.buf.load(Ordering::SeqCst);
        SharedMemoryBuf {
            header: self.header.clone(),
            buf: AtomicPtr::new(bp),
            info: self.info.clone(),
            watchdog: self.watchdog.clone(),
        }
    }
}

// Buffer impls
// - SharedMemoryBuf
impl AsRef<[u8]> for SharedMemoryBuf {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for SharedMemoryBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { self.as_mut_slice_inner() }
    }
}

impl ZSliceBuffer for SharedMemoryBuf {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
    unsafe fn as_mut_slice_unchecked(&mut self) -> &mut [u8] {
        self.as_mut()
    }
    fn as_mut_slice(&mut self) -> Option<&mut [u8]> {
        if self.is_valid() && self.is_unique() {
            return Some(self.as_mut());
        }
        None
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn is_valid(&self) -> bool {
        self.is_valid()
    }
}
