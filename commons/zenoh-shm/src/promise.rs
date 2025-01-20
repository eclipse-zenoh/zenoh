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

use crate::{api::provider::chunk::AllocatedChunk, watchdog::confirmator::ConfirmedDescriptor, ShmBufInfo};

/// A promise to publish SHM buffer
#[zenoh_macros::unstable_doc]
pub struct PromiseInner {
    pub(crate) metadata: ConfirmedDescriptor,
    pub(crate) info: ShmBufInfo,
}

impl PromiseInner {
    pub(crate) fn new(metadata: ConfirmedDescriptor, info: ShmBufInfo) -> Self {
        Self { metadata, info }
    }

    pub(crate) fn publish(&self, chunk: &AllocatedChunk) {
        self.metadata.owned.header().set_data_descriptor(&chunk.descriptor);
    }
}
