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
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
mod shm_tests {
    use zenoh::shm::slice::zsliceshm::zsliceshm;
    use zenoh::shm::slice::zsliceshmmut::zsliceshmmut;
    use zenoh::{
        payload::Payload,
        shm::{
            protocol_implementations::posix::{
                posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend,
                protocol_id::POSIX_PROTOCOL_ID,
            },
            provider::shared_memory_provider::SharedMemoryProviderBuilder,
        },
    };

    #[test]
    fn shm_payload_single_buf() {
        // create an SHM backend...
        let backend = PosixSharedMemoryProviderBackend::builder()
            .with_size(4096)
            .unwrap()
            .res()
            .unwrap();
        // ...and an SHM provider
        let provider = SharedMemoryProviderBuilder::builder()
            .protocol_id::<POSIX_PROTOCOL_ID>()
            .backend(backend)
            .res();

        // Prepare a layout for allocations
        let layout = provider.alloc_layout().size(1024).res().unwrap();

        // allocate an SHM buffer
        let shm_buf = layout.alloc().res().unwrap();

        // build a Payload from an SHM buffer
        let mut payload: Payload = shm_buf.into();

        {
            // deserialize Payload as borrowed ZSliceShm
            let borrowed_shm_buf: &zsliceshm = payload.deserialize().unwrap();
            // construct owned buffer from borrowed type
            let _owned = borrowed_shm_buf.to_owned();
        }

        {
            // deserialize Payload as mutably borrowed ZSliceShm
            let borrowed_shm_buf: &mut zsliceshm = payload.deserialize_mut().unwrap();
            // convert ZSliceShm to ZSliceShmMut
            let _borrowed_shm_buf_mut: &mut zsliceshmmut = borrowed_shm_buf.try_into().unwrap();
        }
    }
}
