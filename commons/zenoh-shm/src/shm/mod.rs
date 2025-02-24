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


#[cfg(target_os="windows")]
mod windows;
#[cfg(target_os="windows")]
use windows as platform;
#[cfg(any(target_os="freebsd", target_os="linux", target_os="macos"))]
mod unix;
#[cfg(any(target_os="freebsd", target_os="linux", target_os="macos"))]
use unix as platform;
#[cfg(all(not(target_os="windows"), not(any(target_os="freebsd", target_os="linux", target_os="macos"))))]
compile_error!("shared_memory isnt implemented for this platform...");

enum SegmentError {
    SegmentExists,
    OsError(u32),
    ZeroSizedSegment
}

pub type ShmResult<T> = core::result::Result<T, SegmentError>;

struct Segment {
    inner: platform::SegmentImpl
}

impl Segment {
    fn create(os_id: String) -> ShmResult<Self> {
        Self {  }
    }
}