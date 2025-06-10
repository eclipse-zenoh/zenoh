//
// Copyright (c) 2025 ZettaScale Technology
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

use zenoh_core::Wait;
use zenoh_shm::api::{
    protocol_implementations::posix::{
        posix_shm_provider_backend::PosixShmProviderBackend, protocol_id::POSIX_PROTOCOL_ID,
    },
    provider::shm_provider::ShmProviderBuilder,
};

#[test]
fn shm_provider_create() {
    let backend = PosixShmProviderBackend::builder()
        .with_size(65536)
        .unwrap()
        .wait()
        .unwrap();
    let _provider = ShmProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .wait();
}

#[test]
fn shm_provider_alloc_and_drop_provider() {
    let backend = PosixShmProviderBackend::builder()
        .with_size(65536)
        .unwrap()
        .wait()
        .unwrap();
    let provider = ShmProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .wait();

    let mut shm_buffer = provider.alloc(1024).wait().unwrap();

    drop(provider);

    shm_buffer[0..1024].fill(0);
}
