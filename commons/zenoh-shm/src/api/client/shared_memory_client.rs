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

use std::fmt::Debug;

use std::sync::Arc;

use zenoh_result::ZResult;

use crate::api::common::types::SegmentID;

use super::shared_memory_segment::SharedMemorySegment;

/// SharedMemoryClient - client factory implementation for particular shared memory protocol
pub trait SharedMemoryClient: Debug + Send + Sync {
    /// Attach to particular shared memory segment
    fn attach(&self, segment: SegmentID) -> ZResult<Arc<dyn SharedMemorySegment>>;
}
