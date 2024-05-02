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
use zenoh::prelude::*;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh_util::try_init_log_from_env();
    run().await.unwrap()
}

async fn run() -> ZResult<()> {
    // create an SHM backend...
    // NOTE: For extended PosixSharedMemoryProviderBackend API please check z_posix_shm_provider.rs
    let backend = PosixSharedMemoryProviderBackend::builder()
        .with_size(65536)
        .unwrap()
        .res()
        .unwrap();
    // ...and an SHM provider
    let provider = SharedMemoryProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .res();

    // Create a layout for particular allocation arguments and particular SHM provider
    // The layout is validated for argument correctness and also is checked
    // against particular SHM provider's layouting capabilities.
    // This layout is reusable and can handle series of similar allocations
    let buffer_layout = {
        // OPTION 1: Simple (default) configuration:
        let simple_layout = shared_memory_provider
            .alloc_layout()
            .size(512)
            .res()
            .unwrap();

        // OPTION 2: Comprehensive configuration:
        let _comprehensive_layout = shared_memory_provider
            .alloc_layout()
            .size(512)
            .alignment(AllocAlignment::new(2))
            .res()
            .unwrap();

        simple_layout
    };

    // Allocate SharedMemoryBuf
    // Policy is a generics-based API to describe necessary allocation behaviour
    // that will be higly optimized at compile-time.
    // Policy resolvable can be sync and async.
    // The basic policies are:
    // -JustAlloc (sync)
    // -GarbageCollect (sync)
    // -Deallocate (sync)
    // --contains own set of dealloc policy generics:
    // ---DeallocateYoungest
    // ---DeallocateEldest
    // ---DeallocateOptimal
    // -BlockOn (sync and async)
    let mut sbuf = async {
        // Some examples on how to use layout's interface:

        // The default allocation with default JustAlloc policy
        let default_alloc = buffer_layout.alloc().res().unwrap();

        // The async allocation
        let _async_alloc = buffer_layout
            .alloc()
            .with_policy::<BlockOn>()
            .res_async()
            .await
            .unwrap();

        // The comprehensive allocation policy that blocks if provider is not able to allocate
        let _comprehensive_alloc = buffer_layout
            .alloc()
            .with_policy::<BlockOn<Defragment<GarbageCollect>>>()
            .res()
            .unwrap();

        // The comprehensive allocation policy that deallocates up to 1000 buffers if provider is not able to allocate
        let _comprehensive_alloc = buffer_layout
            .alloc()
            .with_policy::<Deallocate<1000, Defragment<GarbageCollect>>>()
            .res()
            .unwrap();

        default_alloc
    }
    .await;

    // Fill recently-allocated buffer with data
    sbuf[0..8].fill(0);

    // Declare Session and Publisher (common code)
    let session = zenoh::open(Config::default()).await?;
    let publisher = session.declare_publisher("my/key/expr").await?;

    // Publish SHM buffer
    publisher.put(sbuf).await
}
