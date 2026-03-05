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
//! CUDA device memory support for Zenoh ZSlice transport.
//!
//! Provides [`CudaBufInner`]: a [`ZSliceBuffer`] backed by either:
//! - **Pinned host memory** (`cudaMallocHost`): CPU-writable, DMA-accessible by GPU
//! - **Unified memory** (`cudaMallocManaged`): accessible from both CPU and GPU
//! - **Device-only memory** (`cudaMalloc`): GPU-only; `as_slice()` returns `&[]`
//!
//! The transport codec sends a 64-byte CUDA IPC handle instead of raw bytes,
//! enabling zero-copy GPU tensor sharing between processes on the same host.

use std::{any::Any, fmt};
use zenoh_buffers::ZSliceBuffer;
use zenoh_result::{zerror, ZResult};

/// CUDA IPC memory handle — 64 bytes matching `cudaIpcMemHandle_t`.
pub type CudaIpcHandle = [u8; 64];

/// Discriminates how a [`CudaBufInner`] was allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudaMemKind {
    /// `cudaMallocHost` — pinned host memory. CPU-writable, fast GPU DMA.
    Pinned,
    /// `cudaMallocManaged` — unified memory. CPU and GPU accessible.
    Unified,
    /// `cudaMalloc` — device-only memory. `as_slice()` returns `&[]`.
    Device,
}

/// CUDA Runtime API — error code for success.
const CUDA_SUCCESS: i32 = 0;
/// Flag for `cudaIpcOpenMemHandle`: enable lazy peer access.
const CUDA_IPC_MEM_LAZY_ENABLE_PEER_ACCESS: u32 = 0x01;
/// Flag for `cudaMallocManaged`: attach globally (accessible on all GPUs).
const CUDA_MEM_ATTACH_GLOBAL: u32 = 0x01;

// Raw CUDA Runtime API FFI — linked against libcudart at build time (see build.rs).
extern "C" {
    fn cudaSetDevice(device: i32) -> i32;
    fn cudaMallocHost(ptr: *mut *mut u8, size: usize) -> i32;
    fn cudaFreeHost(ptr: *mut u8) -> i32;
    fn cudaMalloc(ptr: *mut *mut u8, size: usize) -> i32;
    fn cudaMallocManaged(ptr: *mut *mut u8, size: usize, flags: u32) -> i32;
    fn cudaFree(ptr: *mut u8) -> i32;
    fn cudaIpcGetMemHandle(handle: *mut u8, dev_ptr: *mut u8) -> i32;
    fn cudaIpcOpenMemHandle(dev_ptr: *mut *mut u8, handle: *const u8, flags: u32) -> i32;
    fn cudaIpcCloseMemHandle(dev_ptr: *mut u8) -> i32;
}

fn check_cuda(ret: i32, op: &'static str) -> ZResult<()> {
    if ret == CUDA_SUCCESS {
        Ok(())
    } else {
        Err(zerror!("CUDA error {ret} in {op}").into())
    }
}

/// A [`ZSliceBuffer`] backed by CUDA memory (pinned, unified, or device-only).
///
/// The transport codec sends a 64-byte IPC handle instead of raw bytes.
/// On the subscriber side, the handle is opened via `from_ipc`.
pub struct CudaBufInner {
    /// CPU (or null for device-only) pointer to the allocation.
    ptr: *mut u8,
    /// Actual CUDA allocation size in bytes.
    /// For device memory, `ZSlice::len()` returns 0; use `cuda_len()` directly.
    pub cuda_len: usize,
    /// 64-byte CUDA IPC handle identifying the allocation.
    pub ipc_handle: CudaIpcHandle,
    /// CUDA device ordinal that owns this allocation.
    pub device_id: i32,
    /// Memory kind (pinned / unified / device).
    pub mem_kind: CudaMemKind,
    /// True when this buffer was opened via `from_ipc` (subscriber side).
    /// Drop will call `cudaIpcCloseMemHandle` instead of `cudaFree`/`cudaFreeHost`.
    is_ipc_mapping: bool,
    /// False for borrowed pointers (e.g. from torch allocator) — Drop skips cudaFree.
    owns_allocation: bool,
}

// SAFETY: CUDA IPC memory is not aliased between threads when accessed via
// the IPC protocol; the ZSlice Arc ensures single-owner semantics.
unsafe impl Send for CudaBufInner {}
unsafe impl Sync for CudaBufInner {}

impl CudaBufInner {
    /// Allocate pinned host memory (CPU-writable, GPU DMA-accessible).
    ///
    /// Pinned memory is accessible from both CPU and GPU. It is serialized as
    /// raw bytes (not via IPC handle) since it has a valid CPU address.
    pub fn alloc_pinned(len: usize, device_id: i32) -> ZResult<Self> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        unsafe {
            check_cuda(cudaSetDevice(device_id), "cudaSetDevice")?;
            check_cuda(cudaMallocHost(&mut ptr, len), "cudaMallocHost")?;
        }
        Ok(Self {
            ptr,
            cuda_len: len,
            ipc_handle: [0u8; 64], // not used — pinned memory has no IPC handle
            device_id,
            mem_kind: CudaMemKind::Pinned,
            is_ipc_mapping: false,
            owns_allocation: true,
        })
    }

    /// Allocate unified memory (`cudaMallocManaged`).
    ///
    /// Unified memory is CPU- and GPU-accessible. Serialized as raw bytes
    /// (not via IPC handle) since it has a valid CPU address.
    pub fn alloc_unified(len: usize, device_id: i32) -> ZResult<Self> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        unsafe {
            check_cuda(cudaSetDevice(device_id), "cudaSetDevice")?;
            check_cuda(
                cudaMallocManaged(&mut ptr, len, CUDA_MEM_ATTACH_GLOBAL),
                "cudaMallocManaged",
            )?;
        }
        Ok(Self {
            ptr,
            cuda_len: len,
            ipc_handle: [0u8; 64], // not used — unified memory has no IPC handle
            device_id,
            mem_kind: CudaMemKind::Unified,
            is_ipc_mapping: false,
            owns_allocation: true,
        })
    }

    /// Allocate device-only memory (`cudaMalloc`) and capture its IPC handle.
    ///
    /// `as_slice()` returns `&[]` — the buffer is GPU-only. The IPC handle is
    /// sent over the wire so the subscriber can open the allocation directly.
    pub fn alloc_device(len: usize, device_id: i32) -> ZResult<Self> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        unsafe {
            check_cuda(cudaSetDevice(device_id), "cudaSetDevice")?;
            check_cuda(cudaMalloc(&mut ptr, len), "cudaMalloc")?;
        }
        let ipc_handle = Self::get_ipc_handle(ptr, "alloc_device")?;
        Ok(Self {
            ptr,
            cuda_len: len,
            ipc_handle,
            device_id,
            mem_kind: CudaMemKind::Device,
            is_ipc_mapping: false,
            owns_allocation: true,
        })
    }

    /// Wrap an existing device-only allocation (`cudaMalloc`) with its IPC handle.
    ///
    /// Use this when the caller already has a `cudaMalloc`'d pointer (e.g. from
    /// a GPU kernel output) and wants to send it via Zenoh.
    /// The returned buffer **takes ownership** and will call `cudaFree` on drop.
    pub fn from_device_ptr(ptr: *mut u8, len: usize, device_id: i32) -> ZResult<Self> {
        let ipc_handle = Self::get_ipc_handle(ptr, "from_device_ptr")?;
        Ok(Self {
            ptr,
            cuda_len: len,
            ipc_handle,
            device_id,
            mem_kind: CudaMemKind::Device,
            is_ipc_mapping: false,
            owns_allocation: true,
        })
    }

    /// Wrap an externally-owned device pointer **without taking ownership**.
    ///
    /// Use this when the pointer is managed by an external allocator (e.g. the
    /// PyTorch CUDA caching allocator) that must remain responsible for freeing
    /// the memory. Drop will NOT call `cudaFree`.
    ///
    /// The caller must ensure the source allocation outlives this buffer and any
    /// ZBuf/ZSlice referencing it (i.e. until after the publish call completes).
    ///
    /// # Safety
    /// `ptr` must be a valid `cudaMalloc`'d device pointer for `len` bytes on
    /// the given `device_id`. It must remain valid until this `CudaBufInner` drops.
    pub fn from_device_ptr_borrowed(ptr: *mut u8, len: usize, device_id: i32) -> ZResult<Self> {
        let ipc_handle = Self::get_ipc_handle(ptr, "from_device_ptr_borrowed")?;
        Ok(Self {
            ptr,
            cuda_len: len,
            ipc_handle,
            device_id,
            mem_kind: CudaMemKind::Device,
            is_ipc_mapping: false,
            owns_allocation: false,
        })
    }

    /// Reconstruct a buffer on the subscriber side by opening a CUDA IPC handle.
    ///
    /// The returned buffer's `ptr` is a device pointer mapped from the handle.
    /// `as_slice()` returns `&[]`; access must go through GPU kernels.
    pub fn from_ipc(handle: CudaIpcHandle, cuda_len: usize, device_id: i32) -> ZResult<Self> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        unsafe {
            check_cuda(cudaSetDevice(device_id), "cudaSetDevice")?;
            check_cuda(
                cudaIpcOpenMemHandle(
                    &mut ptr,
                    handle.as_ptr(),
                    CUDA_IPC_MEM_LAZY_ENABLE_PEER_ACCESS,
                ),
                "cudaIpcOpenMemHandle",
            )?;
        }
        Ok(Self {
            ptr,
            cuda_len,
            ipc_handle: handle,
            device_id,
            mem_kind: CudaMemKind::Device,
            is_ipc_mapping: true,
            owns_allocation: true,
        })
    }

    /// Return the raw device (or host, for pinned/unified) pointer.
    ///
    /// # Safety
    /// The pointer is valid until this `CudaBufInner` is dropped.
    pub fn as_device_ptr(&self) -> *mut u8 {
        self.ptr
    }

    fn get_ipc_handle(ptr: *mut u8, ctx: &'static str) -> ZResult<CudaIpcHandle> {
        let mut handle = [0u8; 64];
        unsafe {
            check_cuda(cudaIpcGetMemHandle(handle.as_mut_ptr(), ptr), ctx)?;
        }
        Ok(handle)
    }
}

impl ZSliceBuffer for CudaBufInner {
    fn as_slice(&self) -> &[u8] {
        match self.mem_kind {
            CudaMemKind::Pinned | CudaMemKind::Unified => {
                // SAFETY: ptr is valid CPU-accessible memory for the lifetime of self.
                unsafe { std::slice::from_raw_parts(self.ptr, self.cuda_len) }
            }
            CudaMemKind::Device => {
                // Device memory is not CPU-addressable. The codec bypasses as_slice()
                // for ZSliceKind::CudaPtr, so this branch should rarely be reached.
                // Return empty slice rather than panicking to keep ZSlice::new() sane.
                &[]
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Drop for CudaBufInner {
    fn drop(&mut self) {
        if self.ptr.is_null() || !self.owns_allocation {
            return;
        }
        let ret = unsafe {
            match (self.mem_kind, self.is_ipc_mapping) {
                (_, true) => cudaIpcCloseMemHandle(self.ptr),
                (CudaMemKind::Pinned, false) => cudaFreeHost(self.ptr),
                (CudaMemKind::Unified | CudaMemKind::Device, false) => cudaFree(self.ptr),
            }
        };
        if ret != CUDA_SUCCESS {
            tracing::error!("CUDA free error {ret} dropping CudaBufInner");
        }
    }
}

impl fmt::Debug for CudaBufInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CudaBufInner {{ cuda_len: {}, device_id: {}, mem_kind: {:?}, is_ipc: {}, owned: {} }}",
            self.cuda_len, self.device_id, self.mem_kind, self.is_ipc_mapping, self.owns_allocation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use zenoh_buffers::{ZSlice, ZSliceKind};

    #[test]
    #[ignore = "requires CUDA device"]
    fn test_pinned_alloc_roundtrip() {
        let mut buf = CudaBufInner::alloc_pinned(1024, 0).unwrap();
        // Write via CPU
        let slice = unsafe { std::slice::from_raw_parts_mut(buf.as_device_ptr(), buf.cuda_len) };
        slice[0] = 0xDE;
        slice[1] = 0xAD;

        // Read via as_slice()
        assert_eq!(buf.as_slice()[0], 0xDE);
        assert_eq!(buf.as_slice()[1], 0xAD);
    }

    #[test]
    #[ignore = "requires CUDA device"]
    fn test_ipc_handle_serialize() {
        // IPC handles are only valid for device memory (cudaMalloc), not pinned/unified.
        let buf = CudaBufInner::alloc_device(512, 0).unwrap();
        assert_eq!(buf.cuda_len, 512);
        assert_eq!(buf.device_id, 0);
        // IPC handle must be non-zero for real device memory.
        assert_ne!(buf.ipc_handle, [0u8; 64], "IPC handle should be non-zero");

        // Wrap in ZSlice and verify the kind can be set.
        let mut zslice = ZSlice::from(Arc::new(buf));
        zslice.kind = ZSliceKind::CudaPtr;
        assert!(zslice.kind == ZSliceKind::CudaPtr);

        // Note: from_ipc is cross-process by design — opening in the same process
        // will return cudaErrorInvalidResourceHandle (error 1). A full round-trip
        // test requires two separate processes (see examples/cuda_pubsub.rs).
    }
}
