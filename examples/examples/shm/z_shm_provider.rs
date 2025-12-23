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
    shm::{AllocAlignment, PosixShmProviderBackendBinaryHeap, ShmProviderBuilder},
    Wait,
};

fn main() {
    // Option 1: simple way to create default ShmProvider
    {
        // SHM backend (PosixShmProviderBackend)
        let _simple = ShmProviderBuilder::default_backend(4096).wait().unwrap();
    }

    // Option 2: ShmProvider with specific backend
    {
        // Create specific backed
        let backend =
            PosixShmProviderBackendBinaryHeap::builder((4096, AllocAlignment::ALIGN_8_BYTES))
                .wait()
                .unwrap();

        // ...and an SHM provider with specified backend
        let _specific = ShmProviderBuilder::backend(backend).wait();
    }
}
