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

use std::sync::Arc;

use zenoh_result::ZResult;

use crate::api::{
    client::{
        shared_memory_client::SharedMemoryClient, shared_memory_segment::SharedMemorySegment,
    },
    common::types::SegmentID,
};

use super::posix_shared_memory_segment::PosixSharedMemorySegment;

/// Client factory implementation for particular shared memory protocol
#[derive(Debug)]
pub struct PosixSharedMemoryClient;

impl SharedMemoryClient for PosixSharedMemoryClient {
    /// Attach to particular shared memory segment
    fn attach(&self, segment: SegmentID) -> ZResult<Arc<dyn SharedMemorySegment>> {
        Ok(Arc::new(PosixSharedMemorySegment::open(segment)?))
    }
}
