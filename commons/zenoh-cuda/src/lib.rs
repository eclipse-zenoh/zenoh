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

/// FFI wrapper for `cudaIpcMemHandle_t` passed by value.
///
/// The CUDA Runtime API `cudaIpcOpenMemHandle` expects the handle as a
/// by-value struct (64 bytes on the stack per the x86-64 SysV ABI), not
/// as a pointer.  This `#[repr(C)]` newtype lets Rust pass it correctly.
#[repr(C)]
struct CudaIpcMemHandleFfi([u8; 64]);

/// DLPack-compatible tensor metadata for typed GPU tensors.
///
/// Field semantics match DLPack's `DLTensor`:
/// - `dtype_code`: 0=Int, 1=UInt, 2=Float, 4=BFloat16, 5=Complex
/// - `dtype_bits`: element bit width (e.g. 32 for float32)
/// - `dtype_lanes`: number of SIMD lanes (1 for scalar dtypes)
/// - `strides`: None means C-contiguous (no strides on wire)
#[derive(Debug, Clone)]
pub struct TensorMeta {
    pub ndim: i32,
    pub shape: Vec<i64>,
    pub dtype_code: u8,
    pub dtype_bits: u8,
    pub dtype_lanes: u16,
    pub byte_offset: u64,
    pub strides: Option<Vec<i64>>,
}

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
    // NOTE: cudaIpcMemHandle_t is a 64-byte struct passed BY VALUE in C.
    // Using CudaIpcMemHandleFfi (#[repr(C)]) ensures Rust follows the x86-64
    // SysV ABI (MEMORY class → 64 bytes on the stack), not pointer-passing.
    fn cudaIpcOpenMemHandle(dev_ptr: *mut *mut u8, handle: CudaIpcMemHandleFfi, flags: u32) -> i32;
    fn cudaIpcCloseMemHandle(dev_ptr: *mut u8) -> i32;
    // cudaMemcpyKind: 2 = DeviceToHost
    fn cudaMemcpy(dst: *mut u8, src: *const u8, count: usize, kind: u32) -> i32;
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
    /// Optional DLPack tensor metadata (shape, dtype, strides).
    /// Present when this buffer was created with ZSliceKind::CudaTensor.
    tensor_meta: Option<TensorMeta>,
    /// Byte offset from `ptr` (IPC pool base) to the actual tensor data.
    ///
    /// PyTorch's caching allocator sub-allocates from large `cudaMalloc` pool blocks.
    /// `cudaIpcOpenMemHandle` always returns the pool block base, not the tensor's
    /// offset within the block.  `copy_to_host` applies this offset so the correct
    /// bytes are read.  Set from `TensorMeta.byte_offset` on the subscriber side.
    pub byte_offset: u64,
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
            tensor_meta: None,
            byte_offset: 0,
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
            tensor_meta: None,
            byte_offset: 0,
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
            tensor_meta: None,
            byte_offset: 0,
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
            tensor_meta: None,
            byte_offset: 0,
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
            tensor_meta: None,
            byte_offset: 0,
        })
    }

    /// Wrap an externally-owned device pointer with an explicit IPC handle and byte offset.
    ///
    /// Use this when the pointer is sub-allocated from a larger `cudaMalloc` pool block
    /// (e.g. from the PyTorch CUDA caching allocator).  In that case:
    /// - `ipc_handle` identifies the pool block (same handle as its base pointer)
    /// - `byte_offset` = offset from the pool block base to the actual tensor data
    ///
    /// On the subscriber side, `cudaIpcOpenMemHandle` maps the pool block and returns
    /// the pool base.  `copy_to_host` adds `byte_offset` to read the correct bytes.
    pub fn from_device_ptr_borrowed_with_offset(
        ptr: *mut u8,
        len: usize,
        ipc_handle: CudaIpcHandle,
        device_id: i32,
        byte_offset: u64,
    ) -> Self {
        Self {
            ptr,
            cuda_len: len,
            ipc_handle,
            device_id,
            mem_kind: CudaMemKind::Device,
            is_ipc_mapping: false,
            owns_allocation: false,
            tensor_meta: None,
            byte_offset,
        }
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
                    CudaIpcMemHandleFfi(handle),
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
            tensor_meta: None,
            byte_offset: 0,
        })
    }

    /// Reconstruct a typed tensor buffer on the subscriber side by opening a CUDA IPC handle.
    ///
    /// Like `from_ipc`, but also attaches `TensorMeta` so that the receiver can
    /// reconstruct the correct shape and dtype without out-of-band convention.
    /// The `TensorMeta.byte_offset` is applied by `copy_to_host` to account for
    /// sub-allocation offsets within PyTorch's CUDA memory pool blocks.
    pub fn from_ipc_tensor(
        handle: CudaIpcHandle,
        cuda_len: usize,
        device_id: i32,
        meta: TensorMeta,
    ) -> ZResult<Self> {
        let byte_offset = meta.byte_offset;
        let mut base = Self::from_ipc(handle, cuda_len, device_id)?;
        base.byte_offset = byte_offset;
        base.tensor_meta = Some(meta);
        Ok(base)
    }

    /// Attach DLPack tensor metadata to this buffer (builder pattern).
    pub fn with_tensor_meta(mut self, meta: TensorMeta) -> Self {
        self.tensor_meta = Some(meta);
        self
    }

    /// Return the tensor metadata if present.
    pub fn tensor_meta(&self) -> Option<&TensorMeta> {
        self.tensor_meta.as_ref()
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

    /// Copy device memory to a host (CPU) buffer via `cudaMemcpy DeviceToHost`.
    ///
    /// Works for device-only, pinned, and unified memory. For pinned/unified,
    /// a direct `copy_from_slice` on `as_slice()` is cheaper, but this method
    /// works uniformly for all memory kinds and is used by the downgrade path
    /// in [`zenoh_mem_transport::backends::cuda::CudaIpcBackend`].
    ///
    /// `dst` must be at least `self.cuda_len` bytes.
    pub fn copy_to_host(&self, dst: &mut [u8]) -> ZResult<()> {
        if dst.len() < self.cuda_len {
            return Err(zerror!(
                "copy_to_host: dst too small ({} < {})",
                dst.len(),
                self.cuda_len
            )
            .into());
        }
        // Apply byte_offset: ptr is the IPC pool block base; the actual tensor data
        // is at ptr + byte_offset (set from TensorMeta.byte_offset on the wire).
        let src = unsafe { self.ptr.add(self.byte_offset as usize) };
        // cudaMemcpyKind = 2 (DeviceToHost)
        unsafe {
            check_cuda(
                cudaMemcpy(dst.as_mut_ptr(), src as *const u8, self.cuda_len, 2),
                "cudaMemcpy DeviceToHost",
            )?;
        }
        Ok(())
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
