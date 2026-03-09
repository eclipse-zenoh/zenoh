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
use zenoh_buffers::{
    buffer::Buffer,
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};

// ZBuf bounded
macro_rules! zbuf_impl {
    ($bound:ty) => {
        impl LCodec<&ZBuf> for Zenoh080Bounded<$bound> {
            fn w_len(self, message: &ZBuf) -> usize {
                message.len()
            }
        }

        impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            #[inline(always)]
            fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
                self.write(&mut *writer, x.len())?;
                for s in x.zslices() {
                    writer.write_zslice(s)?;
                }
                Ok(())
            }
        }

        impl<R> RCodec<ZBuf, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            #[inline(always)]
            fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
                let len: usize = self.read(&mut *reader)?;
                reader.read_zbuf(len)
            }
        }
    };
}

zbuf_impl!(u8);
zbuf_impl!(u16);
zbuf_impl!(u32);
zbuf_impl!(u64);
zbuf_impl!(usize);

// ZBuf flat
impl<W> WCodec<&ZBuf, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.write(&mut *writer, x)
    }
}

impl<R> RCodec<ZBuf, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.read(&mut *reader)
    }
}

impl LCodec<&ZBuf> for Zenoh080 {
    fn w_len(self, message: &ZBuf) -> usize {
        let zodec = Zenoh080Bounded::<usize>::new();
        zodec.w_len(message)
    }
}

// ZBuf sliced
#[cfg(feature = "shared-memory")]
mod shm {
    use zenoh_buffers::{ZSlice, ZSliceKind};
    use zenoh_shm::ShmBufInner;

    use super::*;
    use crate::Zenoh080Sliced;

    const RAW: u8 = 0;
    const SHM_PTR: u8 = 1;
    #[cfg(feature = "cuda")]
    const CUDA_PTR: u8 = 2;
    #[cfg(feature = "cuda")]
    const CUDA_TENSOR: u8 = 3;

    // Wire size for a CUDA IPC entry: kind(1) + handle(64) + len as u64(8) + device_id as i32(4)
    #[cfg(feature = "cuda")]
    const CUDA_WIRE_SIZE: usize = 1 + 64 + 8 + 4;

    macro_rules! zbuf_sliced_impl {
        ($bound:ty) => {
            impl LCodec<&ZBuf> for Zenoh080Sliced<$bound> {
                fn w_len(self, message: &ZBuf) -> usize {
                    if self.is_sliced {
                        message.zslices().fold(0, |acc, x| {
                            #[cfg(feature = "cuda")]
                            if x.kind == ZSliceKind::CudaPtr || x.kind == ZSliceKind::CudaTensor {
                                // For CudaTensor, we use CUDA_WIRE_SIZE as a minimum estimate.
                                // The actual metadata overhead is small and bounded.
                                return acc + CUDA_WIRE_SIZE;
                            }
                            acc + 1 + x.len()
                        })
                    } else {
                        self.codec.w_len(message)
                    }
                }
            }

            impl<W> WCodec<&ZBuf, &mut W> for Zenoh080Sliced<$bound>
            where
                W: Writer,
            {
                type Output = Result<(), DidntWrite>;

                #[inline(always)]
                fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
                    if self.is_sliced {
                        self.codec.write(&mut *writer, x.zslices().count())?;

                        for zs in x.zslices() {
                            match zs.kind {
                                ZSliceKind::Raw => {
                                    self.codec.write(&mut *writer, RAW)?;
                                    self.codec.write(&mut *writer, zs)?;
                                }
                                ZSliceKind::ShmPtr => {
                                    self.codec.write(&mut *writer, SHM_PTR)?;
                                    let shmb = zs.downcast_ref::<ShmBufInner>().unwrap();
                                    let mut info = vec![];
                                    Zenoh080::new().write(&mut &mut info, &shmb.info)?;
                                    self.codec.write(&mut *writer, &*info)?;
                                    // Increase the reference count so to keep the ShmBufInner
                                    // valid until it is received.
                                    unsafe { shmb.inc_ref_count() };
                                }
                                #[cfg(feature = "cuda")]
                                ZSliceKind::CudaPtr => {
                                    use zenoh_cuda::CudaBufInner;
                                    self.codec.write(&mut *writer, CUDA_PTR)?;
                                    let cuda = zs
                                        .downcast_ref::<CudaBufInner>()
                                        .expect("CudaPtr ZSlice must contain CudaBufInner");
                                    // Send IPC handle + length + device — no bulk data.
                                    writer
                                        .write_exact(&cuda.ipc_handle)
                                        .map_err(|_| DidntWrite)?;
                                    self.codec.write(&mut *writer, cuda.cuda_len as u64)?;
                                    // device_id encoded as u32 (negative values not expected)
                                    self.codec.write(&mut *writer, cuda.device_id as u32)?;
                                }
                                #[cfg(feature = "cuda")]
                                ZSliceKind::CudaTensor => {
                                    use zenoh_cuda::CudaBufInner;
                                    self.codec.write(&mut *writer, CUDA_TENSOR)?;
                                    let cuda = zs
                                        .downcast_ref::<CudaBufInner>()
                                        .expect("CudaTensor ZSlice must contain CudaBufInner");
                                    let meta = cuda
                                        .tensor_meta()
                                        .expect("CudaTensor ZSlice must have TensorMeta");
                                    // IPC handle + len + device_id (same as CudaPtr)
                                    writer
                                        .write_exact(&cuda.ipc_handle)
                                        .map_err(|_| DidntWrite)?;
                                    self.codec.write(&mut *writer, cuda.cuda_len as u64)?;
                                    self.codec.write(&mut *writer, cuda.device_id as u32)?;
                                    // Tensor metadata
                                    self.codec.write(&mut *writer, meta.ndim as u32)?;
                                    for &s in &meta.shape {
                                        self.codec.write(&mut *writer, s as u64)?;
                                    }
                                    self.codec.write(&mut *writer, meta.dtype_code)?;
                                    self.codec.write(&mut *writer, meta.dtype_bits)?;
                                    self.codec.write(&mut *writer, meta.dtype_lanes as u32)?;
                                    self.codec.write(&mut *writer, meta.byte_offset)?;
                                    match &meta.strides {
                                        None => self.codec.write(&mut *writer, 0u8)?,
                                        Some(strides) => {
                                            self.codec.write(&mut *writer, 1u8)?;
                                            for &s in strides {
                                                self.codec.write(&mut *writer, s as u64)?;
                                            }
                                        }
                                    }
                                }
                                // When zenoh-cuda is a workspace member it unconditionally
                                // activates zenoh-buffers/cuda via Cargo feature unification,
                                // making CudaPtr/CudaTensor visible here even when the codec's
                                // own `cuda` feature is not enabled.  Return an error rather
                                // than leaving the match non-exhaustive.
                                #[cfg(not(feature = "cuda"))]
                                _ => return Err(DidntWrite),
                            }
                        }
                    } else {
                        self.codec.write(&mut *writer, x)?;
                    }

                    Ok(())
                }
            }

            impl<R> RCodec<ZBuf, &mut R> for Zenoh080Sliced<$bound>
            where
                R: Reader,
            {
                type Error = DidntRead;

                #[inline(always)]
                fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
                    if self.is_sliced {
                        let num: usize = self.codec.read(&mut *reader)?;
                        let mut zbuf = ZBuf::empty();
                        for _ in 0..num {
                            let kind: u8 = self.codec.read(&mut *reader)?;
                            match kind {
                                RAW => {
                                    let len: usize = self.codec.read(&mut *reader)?;
                                    reader.read_zslices(len, |s| zbuf.push_zslice(s))?;
                                }
                                SHM_PTR => {
                                    let mut zslice: ZSlice = self.codec.read(&mut *reader)?;
                                    zslice.kind = ZSliceKind::ShmPtr;
                                    zbuf.push_zslice(zslice);
                                }
                                #[cfg(feature = "cuda")]
                                CUDA_PTR => {
                                    use std::sync::Arc;
                                    use zenoh_cuda::CudaBufInner;
                                    let mut handle = [0u8; 64];
                                    reader.read_exact(&mut handle).map_err(|_| DidntRead)?;
                                    let len: u64 = self.codec.read(&mut *reader)?;
                                    let device_id: u32 = self.codec.read(&mut *reader)?;
                                    // Open IPC handle — maps the sender's device memory.
                                    let cuda_buf = CudaBufInner::from_ipc(
                                        handle,
                                        len as usize,
                                        device_id as i32,
                                    )
                                    .map_err(|e| {
                                        tracing::error!("CUDA IPC open failed (len={len}, device_id={device_id}): {e}");
                                        DidntRead
                                    })?;
                                    let mut zslice = ZSlice::from(Arc::new(cuda_buf));
                                    zslice.kind = ZSliceKind::CudaPtr;
                                    zbuf.push_zslice(zslice);
                                }
                                #[cfg(feature = "cuda")]
                                CUDA_TENSOR => {
                                    use std::sync::Arc;
                                    use zenoh_cuda::{CudaBufInner, TensorMeta};
                                    let mut handle = [0u8; 64];
                                    reader.read_exact(&mut handle).map_err(|_| DidntRead)?;
                                    let len: u64 = self.codec.read(&mut *reader)?;
                                    let device_id: u32 = self.codec.read(&mut *reader)?;
                                    // Tensor metadata
                                    let ndim: u32 = self.codec.read(&mut *reader)?;
                                    let mut shape: Vec<i64> = Vec::with_capacity(ndim as usize);
                                    for _ in 0..ndim {
                                        let v: u64 = self.codec.read(&mut *reader)?;
                                        shape.push(v as i64);
                                    }
                                    let dtype_code: u8 = self.codec.read(&mut *reader)?;
                                    let dtype_bits: u8 = self.codec.read(&mut *reader)?;
                                    let dtype_lanes: u32 = self.codec.read(&mut *reader)?;
                                    let byte_offset: u64 = self.codec.read(&mut *reader)?;
                                    let strides_present: u8 = self.codec.read(&mut *reader)?;
                                    let strides = if strides_present == 1 {
                                        let mut sv: Vec<i64> = Vec::with_capacity(ndim as usize);
                                        for _ in 0..ndim {
                                            let v: u64 = self.codec.read(&mut *reader)?;
                                            sv.push(v as i64);
                                        }
                                        Some(sv)
                                    } else {
                                        None
                                    };
                                    let meta = TensorMeta {
                                        ndim: ndim as i32,
                                        shape,
                                        dtype_code,
                                        dtype_bits,
                                        dtype_lanes: dtype_lanes as u16,
                                        byte_offset,
                                        strides,
                                    };
                                    let cuda_buf = CudaBufInner::from_ipc_tensor(
                                        handle,
                                        len as usize,
                                        device_id as i32,
                                        meta,
                                    )
                                    .map_err(|e| {
                                        tracing::error!("CUDA_TENSOR IPC open failed (len={len}, device_id={device_id}): {e}");
                                        DidntRead
                                    })?;
                                    let mut zslice = ZSlice::from(Arc::new(cuda_buf));
                                    zslice.kind = ZSliceKind::CudaTensor;
                                    zbuf.push_zslice(zslice);
                                }
                                _ => return Err(DidntRead),
                            }
                        }
                        Ok(zbuf)
                    } else {
                        self.codec.read(&mut *reader)
                    }
                }
            }
        };
    }

    zbuf_sliced_impl!(u8);
    zbuf_sliced_impl!(u16);
    zbuf_sliced_impl!(u32);
    zbuf_sliced_impl!(u64);
    zbuf_sliced_impl!(usize);
}
