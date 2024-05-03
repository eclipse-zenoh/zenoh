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

#[test]
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
fn shm_payload_single_buf() {
    use zenoh::prelude::*;

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
    let mut owned_shm_buf_mut = layout.alloc().res().unwrap();

    // get data
    let _data: &[u8] = &owned_shm_buf_mut;
    let _data_mut: &mut [u8] = &mut owned_shm_buf_mut;

    // convert into immutable owned buffer
    let owned_shm_buf: ZSliceShm = owned_shm_buf_mut.into();

    // get data
    let _data: &[u8] = &owned_shm_buf;

    // convert again into mutable owned buffer
    let mut owned_shm_buf_mut: ZSliceShmMut = owned_shm_buf.try_into().unwrap();

    // get data
    let _data: &[u8] = &owned_shm_buf_mut;
    let _data_mut: &mut [u8] = &mut owned_shm_buf_mut;

    // build a ZBytes from an SHM buffer
    let mut payload: ZBytes = owned_shm_buf_mut.into();

    {
        // deserialize ZBytes as borrowed zsliceshm
        let borrowed_shm_buf: &zsliceshm = payload.deserialize().unwrap();

        // get data
        let _data: &[u8] = borrowed_shm_buf;

        // construct owned buffer from borrowed type
        let owned = borrowed_shm_buf.to_owned();

        // get data
        let _data: &[u8] = &owned;
    }

    {
        // deserialize ZBytes as mutably borrowed zsliceshm
        let borrowed_shm_buf: &mut zsliceshm = payload.deserialize_mut().unwrap();

        // get data
        let _data: &[u8] = borrowed_shm_buf;

        // convert zsliceshm to zsliceshmmut
        let borrowed_shm_buf_mut: &mut zsliceshmmut = borrowed_shm_buf.try_into().unwrap();

        // get data
        let _data: &[u8] = borrowed_shm_buf_mut;
        let _data_mut: &mut [u8] = borrowed_shm_buf_mut;
    }
}
