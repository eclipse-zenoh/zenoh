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

use std::str::FromStr;

use zenoh::{config::Config, prelude::r#async::AsyncResolve, SessionDeclarations};
use zenoh_shm::api::{
    factory::SharedMemoryFactory,
    protocol_implementations::posix::{
        posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend,
        protocol_id::POSIX_PROTOCOL_ID,
    },
};

async fn run_alloc_api() -> zenoh::Result<()> {
    // Initialize factory
    // Even in the factory, the provider is lazily-initialized and does not consume any resources
    // until used (already implemented)
    let mut factory = SharedMemoryFactory::builder()
        .provider(POSIX_PROTOCOL_ID, || {
            Ok(Box::new(PosixSharedMemoryProviderBackend::new(1024)?))
        })?
        .build();

    // Get the SHM provider for POSIX_PROTOCOL_ID
    // The actual resource initialization happens here (already implemented)
    let provider = factory.provider(POSIX_PROTOCOL_ID)?;

    #[derive(Default)]
    struct MyStruct {
        x: usize,
        y: usize,
        z: usize,
    }

    // Allocate String directly in shared memory using SHM provider as an allocator (not implemented)
    let my_object_in_shm = allocator_api2::boxed::Box::try_new_in(MyStruct::default(), provider)?;

    // Declare Session and Publisher (common code)
    let session = zenoh::open(Config::default()).res_async().await?;
    let publisher = session.declare_publisher("my/key/expr").res_async().await?;

    // Publish my_object_in_shm (not implemented)
    // Internally, Zenoh should implement
    // 'Value: From<allocator_api2::boxed::Box<_, SharedMemoryProvider>>'
    // that should use some extension methods for 'allocator_api2::boxed::Box' to interact with Box
    // internals to convert SHM-allocated memory chunk into 'SharedMemoryBuf' to be able to do
    // 'Value: From<SharedMemoryBuf>' for it
    publisher.put(my_object_in_shm).res_async().await
}

async fn run_current_api() -> zenoh::Result<()> {
    // Initialize factory
    // Even in the factory, the provider is lazily-initialized and does not consume any resources
    // until used (already implemented)
    let mut factory = SharedMemoryFactory::builder()
        .provider(POSIX_PROTOCOL_ID, || {
            Ok(Box::new(PosixSharedMemoryProviderBackend::new(1024)?))
        })?
        .build();

    // Get the SHM provider for POSIX_PROTOCOL_ID
    // The actual resource initialization happens here (already implemented)
    let provider = factory.provider(POSIX_PROTOCOL_ID)?;

    // Create String object
    let string = String::from_str("My string")?;
    // ...and get it's slice
    let string_slice = string.as_bytes();

    // Allocate SharedMemoryBuf of desired size
    let mut shm_buf = provider.alloc(string_slice.len()).unwrap();
    {
        // copy String object contents into shm_buf
        let shm_slice = unsafe { shm_buf.as_mut_slice() };
        shm_slice.copy_from_slice(string_slice);
    }

    // Declare Session and Publisher (common code)
    let session = zenoh::open(Config::default()).res_async().await?;
    let publisher = session.declare_publisher("my/key/expr").res_async().await?;

    // Publish shm_buf (already implemented)
    publisher.put(shm_buf).res_async().await
}

#[async_std::main]
async fn main() {
    let _ = run_current_api().await;
    let _ = run_alloc_api().await;
}
