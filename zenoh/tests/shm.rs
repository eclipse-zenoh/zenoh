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
#[cfg(feature = "shared-memory")]
mod tests {
    use async_std::prelude::FutureExt;
    use async_std::task;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use zenoh::prelude::r#async::*;
    use zenoh_core::zasync_executor_init;
    use zenoh_shm::api::factory::SharedMemoryFactory;
    use zenoh_shm::api::protocol_implementations::posix::posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend;
    use zenoh_shm::api::protocol_implementations::posix::protocol_id::POSIX_PROTOCOL_ID;
    use zenoh_shm::api::provider::shared_memory_provider::{BlockOn, GarbageCollect};
    use zenoh_shm::api::provider::types::{AllocAlignment, AllocLayout, MemoryLayout};

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);

    const MSG_COUNT: usize = 1_00;
    const MSG_SIZE: [usize; 2] = [1_024, 100_000];

    macro_rules! ztimeout {
        ($f:expr) => {
            $f.timeout(TIMEOUT).await.unwrap()
        };
    }

    async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
        // Open the sessions
        let mut config = config::peer();
        config.listen.endpoints = endpoints
            .iter()
            .map(|e| e.parse().unwrap())
            .collect::<Vec<_>>();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.transport.shared_memory.set_enabled(true).unwrap();
        println!("[  ][01a] Opening peer01 session: {:?}", endpoints);
        let peer01 = ztimeout!(zenoh::open(config).res_async()).unwrap();

        let mut config = config::peer();
        config.connect.endpoints = endpoints
            .iter()
            .map(|e| e.parse().unwrap())
            .collect::<Vec<_>>();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.transport.shared_memory.set_enabled(true).unwrap();
        println!("[  ][02a] Opening peer02 session: {:?}", endpoints);
        let peer02 = ztimeout!(zenoh::open(config).res_async()).unwrap();

        (peer01, peer02)
    }

    async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
        // Open the sessions
        let mut config = config::peer();
        config.listen.endpoints = vec![endpoint01.parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(true)).unwrap();
        config.transport.shared_memory.set_enabled(true).unwrap();
        println!("[  ][01a] Opening peer01 session: {}", endpoint01);
        let peer01 = ztimeout!(zenoh::open(config).res_async()).unwrap();

        let mut config = config::peer();
        config.listen.endpoints = vec![endpoint02.parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(true)).unwrap();
        config.transport.shared_memory.set_enabled(true).unwrap();
        println!("[  ][02a] Opening peer02 session: {}", endpoint02);
        let peer02 = ztimeout!(zenoh::open(config).res_async()).unwrap();

        (peer01, peer02)
    }

    async fn close_session(peer01: Session, peer02: Session) {
        println!("[  ][01d] Closing peer02 session");
        ztimeout!(peer01.close().res_async()).unwrap();
        println!("[  ][02d] Closing peer02 session");
        ztimeout!(peer02.close().res_async()).unwrap();
    }

    async fn test_session_pubsub(peer01: &Session, peer02: &Session, reliability: Reliability) {
        let msg_count = match reliability {
            Reliability::Reliable => MSG_COUNT,
            Reliability::BestEffort => 1,
        };
        let msgs = Arc::new(AtomicUsize::new(0));

        for size in MSG_SIZE {
            let key_expr = format!("shm{size}");

            msgs.store(0, Ordering::SeqCst);

            // Subscribe to data
            println!("[PS][01b] Subscribing on peer01 session");
            let c_msgs = msgs.clone();
            let _sub = ztimeout!(peer01
                .declare_subscriber(&key_expr)
                .callback(move |sample| {
                    assert_eq!(sample.value.payload.len(), size);
                    c_msgs.fetch_add(1, Ordering::Relaxed);
                })
                .res_async())
            .unwrap();

            // Wait for the declaration to propagate
            task::sleep(SLEEP).await;

            // Make factory with POSIX SHM provider
            let c_size = size;
            let mut factory = SharedMemoryFactory::builder()
                .provider(POSIX_PROTOCOL_ID, move || {
                    Ok(Box::new(
                        PosixSharedMemoryProviderBackend::builder()
                            .with_size(c_size * MSG_COUNT / 10)?
                            .res()?,
                    ))
                })
                .unwrap()
                .build();
            let shm01 = factory.provider(POSIX_PROTOCOL_ID).unwrap();

            // remember segment size that was allocated
            let shm_segment_size = shm01.available();

            // Put data
            println!("[PS][03b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");

            let layout = AllocLayout::new(size, AllocAlignment::default(), shm01).unwrap();

            for c in 0..msg_count {
                // Create the message to send
                let sbuf = ztimeout!(shm01
                    .alloc()
                    .with_policy::<BlockOn<GarbageCollect>>()
                    .with_layout(&layout)
                    .res_async())
                .unwrap();
                println!("{c} created");

                ztimeout!(peer02
                    .put(&key_expr, sbuf)
                    .congestion_control(CongestionControl::Block)
                    .res_async())
                .unwrap();
                println!("{c} putted");
            }

            // wat for all messages received
            ztimeout!(async {
                loop {
                    let cnt = msgs.load(Ordering::Relaxed);
                    println!("[PS][03b] Received {cnt}/{msg_count}.");
                    if cnt != msg_count {
                        task::sleep(SLEEP).await;
                    } else {
                        break;
                    }
                }
            });

            // wat for all memory reclaimed
            ztimeout!(async {
                loop {
                    shm01.garbage_collect();
                    let available = shm01.available();
                    println!("[PS][03b] SHM available {available}/{shm_segment_size}");
                    if available != shm_segment_size {
                        task::sleep(SLEEP).await;
                    } else {
                        break;
                    }
                }
            });
        }
    }

    #[cfg(feature = "shared-memory")]
    #[test]
    fn zenoh_shm_unicast() {
        task::block_on(async {
            zasync_executor_init!();
            let _ = env_logger::try_init();

            let (peer01, peer02) = open_session_unicast(&["tcp/127.0.0.1:17447"]).await;
            test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
            close_session(peer01, peer02).await;
        });
    }

    #[cfg(feature = "shared-memory")]
    #[test]
    fn zenoh_shm_multicast() {
        task::block_on(async {
            zasync_executor_init!();
            let _ = env_logger::try_init();

            let (peer01, peer02) =
                open_session_multicast("udp/224.0.0.1:17448", "udp/224.0.0.1:17448").await;
            test_session_pubsub(&peer01, &peer02, Reliability::BestEffort).await;
            close_session(peer01, peer02).await;
        });
    }

    #[cfg(feature = "shared-memory")]
    #[test]
    fn shm_api_example() {
        use zenoh_result::{bail, ZResult};

        let _ = task::block_on::<_, ZResult<()>>(async {
            zasync_executor_init!();
            let _ = env_logger::try_init();

            // Initialize the SHM factory
            // In this example the provider is lazily-initialized and does not consume any resources until used
            // It is up to the user to use lazy-initialization or move an already-initialized backend instance
            // into factory's initialization lambda
            let mut factory = SharedMemoryFactory::builder()
                .provider(POSIX_PROTOCOL_ID, || {
                    // NOTE: code in this block is a specific PosixSharedMemoryProviderBackend API.
                    // The initialisation of SHM backend is completely backend-specific and user is free to do
                    // anything reasonable here. This code is execuated at the provider's first use

                    // Alignment for POSIX SHM provider
                    // All allocations will be aligned corresponding to this alignment -
                    // that means that the provider will be able to satisfy allocation layouts
                    // with alignment <= provider_alignment
                    let provider_alignment = AllocAlignment::default();

                    // Create layout for POSIX Provider's memory
                    let provider_layout = MemoryLayout::new(1024, provider_alignment).unwrap();

                    // Construct and return the backend with specified memory layout
                    Ok(Box::new(
                        PosixSharedMemoryProviderBackend::builder()
                            .with_layout(provider_layout)
                            .res()?,
                    ))
                })?
                .build();

            // Get the SHM provider for POSIX_PROTOCOL_ID
            // The actual resource initialization happens here
            let provider = factory.provider(POSIX_PROTOCOL_ID)?;

            // Create a layout for particular buffer and particular SHM provider
            // The layout is validated for argument correctness and also is checked
            // against particular SHM provider's layouting capabilities. This approach
            // allows making a reusable layout for series of similar allocations
            let buffer_layout = AllocLayout::new(512, AllocAlignment::default(), provider).unwrap();

            // Allocate SharedMemoryBuf using asynchronous BlockOn<GarbageCollect<JustAlloc>> policy.
            // Policy generics collection is a mechanism to describe necessary allocation behaviour
            // that will be higly optimized at compile-time.
            // The basic policies are:
            // -JustAlloc (sync)
            // -GarbageCollect (sync)
            // -Deallocate (sync)
            // --contains own set of dealloc policy generics:
            // ---DeallocateYoungest
            // ---DeallocateEldest
            // ---DeallocateOptimal
            // -BlockOn (sync and async)
            // SharedMemoryProvider's alloc supports sync and async APIs to do the allocations
            let sbuf = ztimeout!(provider
                .alloc()
                .with_policy::<BlockOn<GarbageCollect>>()
                .with_layout(&buffer_layout)
                .res_async())
            .unwrap();

            // Declare Session and Publisher (common code)
            // Session and Publisher do not depend from SHM subsystem...
            let session = zenoh::open(Config::default()).res_async().await?;
            let publisher = session.declare_publisher("my/key/expr").res_async().await?;

            // Publish sbuf (not implemented)
            let _ = publisher.put(sbuf).res_async().await;

            bail!("Finished...")
        });
    }
}
