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

use zenoh_shm::api::{
    client::shared_memory_client::SharedMemoryClient,
    protocol_implementations::posix::{
        posix_shared_memory_client::PosixSharedMemoryClient,
        posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend,
    },
    provider::shared_memory_provider_backend::SharedMemoryProviderBackend,
};

static BUFFER_NUM: u32 = 100;
static BUFFER_SIZE: u32 = 1024;

#[test]
fn posix_shm_provider_create() {
    let _backend = PosixSharedMemoryProviderBackend::new(1024)
        .expect("Error creating PosixSharedMemoryProviderBackend!");
}

#[test]
fn posix_shm_provider_alloc() {
    let mut backend = PosixSharedMemoryProviderBackend::new(1024)
        .expect("Error creating PosixSharedMemoryProviderBackend!");

    let _buf = backend
        .alloc(100)
        .expect("PosixSharedMemoryProviderBackend: error allocating buffer");
}

#[test]
fn posix_shm_provider_open() {
    let mut backend = PosixSharedMemoryProviderBackend::new(1024)
        .expect("Error creating PosixSharedMemoryProviderBackend!");

    let buf = backend
        .alloc(100)
        .expect("PosixSharedMemoryProviderBackend: error allocating buffer");

    let client = PosixSharedMemoryClient {};

    let _segment = client
        .attach(buf.descriptor.segment)
        .expect("Error attaching to segment");
}

#[test]
fn posix_shm_provider_allocator() {
    let mut backend = PosixSharedMemoryProviderBackend::new(BUFFER_NUM * BUFFER_SIZE)
        .expect("Error creating PosixSharedMemoryProviderBackend!");

    // exaust memory by allocating it all
    let mut buffers = vec![];
    for _ in 0..BUFFER_NUM {
        let buf = backend
            .alloc(BUFFER_SIZE as usize)
            .expect("PosixSharedMemoryProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    
    for _ in 0..BUFFER_NUM {
        // there is nothing to allocate at this point
        assert_eq!(backend.available(), 0);
        assert!(backend.alloc(BUFFER_SIZE as usize).is_err());

        // free buffer
        let to_free = buffers.pop().unwrap().descriptor;
        backend.free(&to_free);

        // allocate new one
        let buf = backend
            .alloc(BUFFER_SIZE as usize)
            .expect("PosixSharedMemoryProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    // free everything
    while let Some(buffer) = buffers.pop() {
        backend.free(&buffer.descriptor);
    }    

    // confirm that allocator is free
    assert_eq!(backend.available(), (BUFFER_NUM * BUFFER_SIZE) as usize);
}
