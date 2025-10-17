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
use zenoh::{
    shm::{AllocAlignment, MemoryLayout, PosixShmProviderBackend, ShmProviderBuilder},
    Wait,
};

fn main() {
    // Construct an SHM backend
    let backend = {
        // NOTE: code in this block is a specific PosixShmProviderBackend API.

        // Total amount of shared memory to allocate
        let size = 4096;

        // An alignment for POSIX SHM provider
        // Due to internal optimization, all allocations will be aligned corresponding to this alignment,
        // so the provider will be able to satisfy allocation layouts with alignment <= provider_alignment
        let provider_alignment = AllocAlignment::default();

        // A layout for POSIX Provider's memory
        let provider_layout = MemoryLayout::new(size, provider_alignment).unwrap();

        // Build a provider backend
        PosixShmProviderBackend::builder(&provider_layout)
            .wait()
            .unwrap()
    };

    // Construct an SHM provider for particular backend
    let _shm_provider = ShmProviderBuilder::backend(backend).wait();
}
