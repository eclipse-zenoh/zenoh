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
    shm::{
        AllocAlignment, BlockOn, Deallocate, Defragment, GarbageCollect, PosixShmProviderBackend,
        ShmProviderBuilder,
    },
    Config, Wait,
};

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");
    run_minimal().await.unwrap();
    run().await.unwrap();
}

async fn run_minimal() -> zenoh::Result<()> {
    let provider = ShmProviderBuilder::default_backend().wait()?;
    let mut sbuf = provider.alloc(512).wait().unwrap();

    // Fill recently-allocated buffer with data
    sbuf[0..8].fill(0);

    // Declare Session and Publisher (common code)
    let session = zenoh::open(Config::default()).await?;
    let publisher = session.declare_publisher("my/key/expr").await?;

    // Publish SHM buffer
    publisher.put(sbuf).await
}

async fn run() -> zenoh::Result<()> {
    // create an SHM provider
    // there is a simple way to create default ShmProvider initialized with default-configured
    // SHM backend (PosixShmProviderBackend) and more comprehensive way
    let provider = {
        // OPTION: simple ShmProvider creation
        let _simple = ShmProviderBuilder::default_backend().wait().unwrap();

        // OPTION: comprehensive ShmProvider creation
        // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
        // create backend...
        let comprehencive = PosixShmProviderBackend::builder()
            .with_size(65536)
            .unwrap()
            .wait()
            .unwrap();
        // ...and an SHM provider
        ShmProviderBuilder::backend(comprehencive).wait()
    };

    // There are two API-defined ways of making shm buffer allocations: direct and through the layout...

    // Direct allocation
    // The direct allocation calculates all layouting checks on each allocation. It is good for making
    // uniquely-layouted allocations. For making series of similar allocations, please refer to  layout
    // allocation API which is shown later in this example...
    let _direct_allocation = {
        // OPTION: Simple allocation
        let simple = provider.alloc(512).wait().unwrap();

        // OPTION: Allocation with custom alignment and alloc policy customization
        let _comprehensive = provider
            .alloc(512)
            .with_alignment(AllocAlignment::new(2).unwrap())
            // for more examples on policies, please see allocation policy usage below (for layout allocation API)
            .with_policy::<GarbageCollect>()
            .wait()
            .unwrap();

        // OPTION: Allocation with custom alignment and async alloc policy
        let _async = provider
            .alloc(512)
            .with_alignment(AllocAlignment::new(2).unwrap())
            // for more examples on policies, please see allocation policy usage below (for layout allocation API)
            .with_policy::<BlockOn<Defragment<GarbageCollect>>>()
            .await
            .unwrap();

        simple
    };

    // Create a layout for particular allocation arguments and particular SHM provider
    // The layout is validated for argument correctness and also is checked
    // against particular SHM provider's layouting capabilities.
    // This layout is reusable and can handle series of similar allocations
    let buffer_layout = {
        // OPTION: Simple configuration:
        let simple_layout = provider.alloc(512).into_layout().unwrap();

        // OPTION: Comprehensive configuration:
        let _comprehensive_layout = provider
            .alloc(512)
            .with_alignment(AllocAlignment::new(2).unwrap())
            .into_layout()
            .unwrap();

        simple_layout
    };

    // Allocate ShmBufInner
    // Policy is a generics-based API to describe necessary allocation behaviour
    // that will be highly optimized at compile-time.
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

        // OPTION: The default allocation with default JustAlloc policy
        let default_alloc = buffer_layout.alloc().wait().unwrap();

        // OPTION: The async allocation
        let _async_alloc = buffer_layout
            .alloc()
            .with_policy::<BlockOn>()
            .await
            .unwrap();

        // OPTION: The comprehensive allocation policy that blocks if provider is not able to allocate
        let _comprehensive_alloc = buffer_layout
            .alloc()
            .with_policy::<BlockOn<Defragment<GarbageCollect>>>()
            .wait()
            .unwrap();

        // OPTION: The comprehensive allocation policy that deallocates up to 1000 buffers if provider is not able to allocate
        let _comprehensive_alloc = buffer_layout
            .alloc()
            .with_policy::<Deallocate<1000, Defragment<GarbageCollect>>>()
            .wait()
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
