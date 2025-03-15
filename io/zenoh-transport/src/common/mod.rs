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
use zenoh_buffers::ZSlice;

pub mod batch;
pub(crate) mod defragmentation;
pub(crate) mod pipeline;
pub(crate) mod priority;
pub(crate) mod seq_num;
#[cfg(feature = "stats")]
pub mod stats;

/// Read bytes asynchronously into the buffer if it is not shared, or replacing it
/// with a new allocated one otherwise.
pub(crate) fn read_with_buffer<'a, F>(
    buffer: &'a mut ZSlice,
    read: impl FnOnce(&'a mut [u8]) -> F,
) -> F {
    // SAFETY: the buffer slice range is not modified by read
    unsafe {
        if let Some(buf) = buffer.downcast_mut::<Vec<u8>>() {
            // use an intermediate to circumvent current borrow checker limitation
            return read(&mut *(buf.as_mut_slice() as *mut [u8]));
        }
        *buffer = vec![0u8; buffer.capacity()].into();
        read(buffer.downcast_mut::<Vec<u8>>().unwrap())
    }
}
