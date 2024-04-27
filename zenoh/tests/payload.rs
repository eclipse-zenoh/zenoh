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
    use zenoh::prelude::r#async::*;

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

    // allocate an SHM buffer (ZSliceShmMut)
    let owned_shm_buf_mut = layout.alloc().res().unwrap();

    // convert into immutable owned buffer (ZSliceShmMut -> ZSlceShm)
    let owned_shm_buf: ZSliceShm = owned_shm_buf_mut.into();

    // convert again into mutable owned buffer (ZSliceShm -> ZSlceShmMut)
    let owned_shm_buf_mut: ZSliceShmMut = owned_shm_buf.try_into().unwrap();

    // build a ZBytes from an SHM buffer (ZSliceShmMut -> ZBytes)
    let mut payload: ZBytes = owned_shm_buf_mut.into();

    // branch to illustrate immutable access to SHM data
    {
        // deserialize ZBytes as an immutably borrowed zsliceshm (ZBytes -> &zsliceshm)
        let borrowed_shm_buf: &zsliceshm = payload.deserialize().unwrap();

        // construct owned buffer from borrowed type (&zsliceshm -> ZSliceShm)
        let owned = borrowed_shm_buf.to_owned();

        // try to construct mutable ZSliceShmMut (ZSliceShm -> ZSliceShmMut)
        let owned_mut: Result<ZSliceShmMut, _> = owned.try_into();
        // the attempt fails because ZSliceShm has two existing references ('owned' and inside 'payload')
        assert!(owned_mut.is_err())
    }

    // branch to illustrate mutable access to SHM data
    {
        // deserialize ZBytes as mutably borrowed zsliceshm (ZBytes -> &mut zsliceshm)
        let borrowed_shm_buf: &mut zsliceshm = payload.deserialize_mut().unwrap();

        // convert zsliceshm to zsliceshmmut (&mut zsliceshm -> &mut zsliceshmmut)
        let _borrowed_shm_buf_mut: &mut zsliceshmmut = borrowed_shm_buf.try_into().unwrap();
    }
}
