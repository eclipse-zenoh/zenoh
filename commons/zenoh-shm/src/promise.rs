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
    ptr::null_mut,
    sync::{atomic::AtomicPtr, Arc},
};

use crate::{
    api::{buffer::pbw::ZPromise, provider::chunk::AllocatedChunk},
    watchdog::confirmator::ConfirmedDescriptor,
    ShmBufInfo, ShmBufInner,
};

impl Into<ShmBufInner> for &ZPromise {
    fn into(self) -> ShmBufInner {
        unsafe { self.0.into_shm_buf_inner() }
    }
}

/// An inner promise to publish SHM buffer
#[zenoh_macros::unstable_doc]
pub struct PromiseInner {
    pub(crate) metadata: Arc<ConfirmedDescriptor>,
    pub(crate) info: ShmBufInfo,
}

impl PromiseInner {
    pub(crate) fn new(metadata: Arc<ConfirmedDescriptor>, info: ShmBufInfo) -> Self {
        Self { metadata, info }
    }

    pub(crate) fn publish(&self, chunk: &AllocatedChunk) {
        self.metadata
            .owned
            .header()
            .set_data_descriptor(&chunk.descriptor);
    }

    unsafe fn into_shm_buf_inner(&self) -> ShmBufInner {
        ShmBufInner {
            metadata: self.metadata.clone(),
            info: self.info.clone(),
            buf: AtomicPtr::<u8>::new(null_mut()),
        }
    }
}
