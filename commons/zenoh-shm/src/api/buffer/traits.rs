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
use std::{num::NonZeroUsize, ptr};

use crate::api::provider::memory_layout::MemoryLayout;

pub trait ResideInShm:
    Send + zerocopy::KnownLayout + zerocopy::FromBytes + zerocopy::IntoBytes
{
}

impl<T: Send + zerocopy::KnownLayout + zerocopy::FromBytes + zerocopy::IntoBytes> ResideInShm
    for T
{
}

/// Errors for buffer relayouting operation.
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub enum BufferRelayoutError {
    IncompatibleAlignment,
    SizeTooBig,
}

#[zenoh_macros::unstable_doc]
pub trait ShmBuf<T: ?Sized>: Sized + AsRef<T> {
    #[zenoh_macros::unstable_doc]
    fn is_valid(&self) -> bool;
}

pub trait ShmConvert<T: ?Sized, Tother: ?Sized> {
    /// Performs the conversion.
    fn shm_convert<Tdst: ShmBuf<Tother>>(self) -> Result<Tdst, Self>
    where
        Self: ShmBuf<T>;
}

impl<T: ResideInShm, Tsrc: ShmBuf<T>> ShmConvert<T, [u8]> for Tsrc {
    fn shm_convert<Tdst: ShmBuf<[u8]>>(self) -> Result<Tdst, Self> {
        // fully manually morph self into Tdst
        let self_ptr = (&self as *const Self) as *const Tdst;
        let new_self = unsafe { ptr::read::<Tdst>(self_ptr) };
        std::mem::forget(self);

        Ok(new_self)
    }
}

impl<Tsrc: ShmBuf<[u8]>, Tother: ResideInShm> ShmConvert<[u8], Tother> for Tsrc {
    fn shm_convert<Tdst: ShmBuf<Tother>>(self) -> Result<Tdst, Self> {
        // layout checks block
        {
            // check size
            let slice = self.as_ref();
            let type_size = std::mem::size_of::<Tother>();
            let size = slice.len();
            if type_size != size {
                return Err(self);
            }

            // check alignment
            let ptr = slice.as_ptr();
            let type_align = std::mem::align_of::<Tother>();
            if ((ptr as usize) % type_align) != 0 {
                return Err(self);
            }
        }

        // fully manually morph self into Tdst
        let self_ptr = (&self as *const Self) as *const Tdst;
        let new_self = unsafe { ptr::read::<Tdst>(self_ptr) };
        std::mem::forget(self);

        Ok(new_self)
    }
}

#[zenoh_macros::unstable_doc]
pub trait ShmBufUnsafeMut<T: ?Sized>: ShmBuf<T> {
    #[zenoh_macros::unstable_doc]
    /// Get unchecked mutable access to buffer's memory.
    ///
    /// This is unsafe yet very powerful API for building concurrent access logic around SHM buffer contents.
    /// For safe version please see `ShmBufMut` trait.
    ///
    /// # Safety
    ///
    /// Safe if multiple conditions are met:
    /// - user code guarantees no data race across all applications that share the buffer
    /// - the buffer is not being concurrently sent to the outside of SHM domain
    /// - the buffer is valid
    unsafe fn as_mut_unchecked(&mut self) -> &mut T;
}

#[zenoh_macros::unstable_doc]
pub trait ShmBufMut<T: ?Sized>: ShmBuf<T> + AsMut<T> {}

#[zenoh_macros::unstable_doc]
pub trait OwnedShmBuf<T: ?Sized>: ShmBuf<T> {
    #[zenoh_macros::unstable_doc]
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()>;

    #[zenoh_macros::unstable_doc]
    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError>;
}
