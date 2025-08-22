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
use std::sync::atomic::AtomicUsize;

use zenoh::{
    shm::{
        AllocAlignment, BlockOn, BuildLayout, Deallocate, Defragment, GarbageCollect,
        PosixShmProviderBackend, ResideInShm, ShmProviderBuilder,
    },
    Config, Wait,
};

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");
    run().await.unwrap();
}

async fn run() -> zenoh::Result<()> {
    // Create an SHM provider
    let provider = {
        // Option 1: simple way to create default ShmProvider initialized with default-configured
        {
            // SHM backend (PosixShmProviderBackend)
            let _simple = ShmProviderBuilder::default_backend(65536).wait()?;
        }

        // Option 2: comprehensive ShmProvider creation
        {
            // Create specific backed
            // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
            let comprehensive =
                PosixShmProviderBackend::builder((65536, AllocAlignment::ALIGN_8_BYTES)).wait()?;

            // ...and an SHM provider with specified backend
            ShmProviderBuilder::backend(comprehensive).wait()
        }
    };

    // Allocate SHM buffer

    // There are two ways of making shm buffer allocations: direct and through the layout...

    // Option 1: direct allocation
    // The direct allocation calculates all layouting checks on each allocation. It is good for making
    // uniquely-layouted allocations.
    {
        // Option 1: Simple allocation
        let _shm_buf = provider.alloc(512).wait()?;

        // Option 2: Allocation with custom alignment
        let _shm_buf = provider
            .alloc((512, AllocAlignment::ALIGN_2_BYTES))
            .wait()?;
    };

    // Option 2: allocation layout
    // Layout is reusable and is designed to handle series of similar allocations
    {
        // Option 1: Simple configuration:
        let simple_layout = provider.alloc(512).into_layout()?;
        let _shm_buf = simple_layout.alloc().wait()?;

        // Option 2: Comprehensive configuration:
        let comprehensive_layout = provider
            .alloc((512, AllocAlignment::ALIGN_2_BYTES))
            .into_layout()?;
        let _shm_buf = comprehensive_layout.alloc().wait()?;
    };

    // Typed allocation
    {
        // Shared data
        #[repr(C)]
        pub struct SharedData {
            pub len: AtomicUsize,
            pub data: [u8; 1024],
        }

        // #SAFETY: this is safe because SharedData is safe to be shared
        unsafe impl ResideInShm for SharedData {}

        // typed layout
        let typed_layout = BuildLayout::for_type::<SharedData>();

        // allocate **typed** SHM buffer
        let _typed_buf = provider.alloc(typed_layout).wait().unwrap();

        // allocate **untyped** SHM buffer
        let _untyped_buf = provider.alloc(typed_layout.layout()).wait().unwrap();
    }

    // Allocation policies
    // Policy is a generics-based API to describe necessary allocation behaviour to be optimized at compile-time.
    // Policy can be sync and async.
    // The basic policies are:
    // -JustAlloc (sync)
    // -GarbageCollect (sync)
    // -Deallocate (sync)
    // --contains own set of dealloc policy generics:
    // ---DeallocateYoungest
    // ---DeallocateEldest
    // ---DeallocateOptimal
    // -BlockOn (sync and async)
    let mut sbuf = {
        // Option: The default allocation with default JustAlloc policy
        let default_alloc = provider.alloc(512).wait()?;

        // Option: Defragment and garbage collect if there is not enough shared memory for allocation
        let _gc_defragment_alloc = provider
            .alloc(512)
            .with_policy::<Defragment<GarbageCollect>>()
            .wait()?;

        // Option: Sync block if there is not enough shared memory for allocation
        let _sync_alloc = provider.alloc(512).with_policy::<BlockOn>().wait()?;

        // Option: Async block if there is not enough shared memory for allocation
        let _async_alloc = provider.alloc(512).with_policy::<BlockOn>().await?;

        // Option: The comprehensive allocation policy that tries to GC, defragment and then blocks
        // if provider is not able to allocate
        let _comprehensive_alloc = provider
            .alloc(512)
            .with_policy::<BlockOn<Defragment<GarbageCollect>>>()
            .await?;

        // Option: The comprehensive allocation policy that tries to GC, defragment and then deallocates up to
        // 10 buffers if provider is not able to allocate
        let _comprehensive_alloc = provider
            .alloc(512)
            .with_policy::<Deallocate<10, Defragment<GarbageCollect>>>()
            .wait()?;

        default_alloc
    };

    // Fill recently-allocated buffer with data
    sbuf[0..8].fill(0);

    // Declare Session and Publisher (common code)
    let session = zenoh::open(Config::default()).await?;
    let publisher = session.declare_publisher("my/key/expr").await?;

    // Publish SHM buffer
    publisher.put(sbuf).await
}
