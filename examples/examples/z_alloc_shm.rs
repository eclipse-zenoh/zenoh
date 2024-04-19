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
use zenoh::prelude::r#async::*;
use zenoh::shm::protocol_implementations::posix::posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend;
use zenoh::shm::protocol_implementations::posix::protocol_id::POSIX_PROTOCOL_ID;
use zenoh::shm::provider::shared_memory_provider::{
    BlockOn, GarbageCollect, SharedMemoryProviderBuilder,
};
use zenoh::shm::provider::shared_memory_provider::{Deallocate, Defragment};
use zenoh::shm::provider::types::{AllocAlignment, MemoryLayout};
use zenoh::Result;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh_util::try_init_log_from_env();
    run().await.unwrap()
}

async fn run() -> Result<()> {
    // Construct an SHM backend
    let backend = {
        // NOTE: code in this block is a specific PosixSharedMemoryProviderBackend API.
        // The initialisation of SHM backend is completely backend-specific and user is free to do
        // anything reasonable here. This code is execuated at the provider's first use

        // Alignment for POSIX SHM provider
        // All allocations will be aligned corresponding to this alignment -
        // that means that the provider will be able to satisfy allocation layouts
        // with alignment <= provider_alignment
        let provider_alignment = AllocAlignment::default();

        // Create layout for POSIX Provider's memory
        let provider_layout = MemoryLayout::new(65536, provider_alignment).unwrap();

        PosixSharedMemoryProviderBackend::builder()
            .with_layout(provider_layout)
            .res()
            .unwrap()
    };

    // Construct an SHM provider for particular backend and POSIX_PROTOCOL_ID
    let shared_memory_provider = SharedMemoryProviderBuilder::builder()
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
    let session = zenoh::open(Config::default()).res_async().await?;
    let publisher = session.declare_publisher("my/key/expr").res_async().await?;

    // Publish SHM buffer
    publisher.put(sbuf).res_async().await
}
