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

use std::sync::atomic::{AtomicBool, AtomicU32};

// Chunk header
#[stabby::stabby]
#[derive(Debug)]
pub struct ChunkHeaderType {
    /*
    TODO: We don't really need 32 bits here, but access to 16-bit felds with 1 byte alignment is less performant on most of the platforms.
    We need to bench and select reasonable integer sizes here once we have an implementation to bench
    */
    pub refcount: AtomicU32,
    pub watchdog_invalidated: AtomicBool,
    pub generation: AtomicU32,
}
