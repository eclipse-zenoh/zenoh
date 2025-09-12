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
#![cfg(all(feature = "shared-memory", feature = "unstable"))]
use zenoh::{
    bytes::ZBytes,
    shm::{zshmmut, PosixShmProviderBackend, ShmProviderBuilder, ZShm, ZShmMut},
    Wait,
};

#[test]
fn shm_bytes_single_buf() {
    // create an SHM backend...
    let backend = PosixShmProviderBackend::builder(4096).wait().unwrap();
    // ...and an SHM provider
    let provider = ShmProviderBuilder::backend(backend).wait();

    // Prepare a layout for allocations
    let layout = provider.alloc_layout(1024).unwrap();

    // allocate an SHM buffer (ZShmMut)
    let owned_shm_buf_mut = layout.alloc().wait().unwrap();

    // convert into immutable owned buffer (ZShmMut -> ZSlceShm)
    let owned_shm_buf: ZShm = owned_shm_buf_mut.into();

    // convert again into mutable owned buffer (ZShm -> ZSlceShmMut)
    let owned_shm_buf_mut: ZShmMut = owned_shm_buf.try_into().unwrap();

    // build a ZBytes from an SHM buffer (ZShmMut -> ZBytes)
    let mut payload: ZBytes = owned_shm_buf_mut.into();

    // branch to illustrate immutable access to SHM data
    {
        // deserialize ZBytes as an immutably borrowed zshm (ZBytes -> &zshm)
        let borrowed_shm_buf = payload.as_shm().unwrap();

        // construct owned buffer from borrowed type (&zshm -> ZShm)
        let owned = borrowed_shm_buf.to_owned();

        // try to construct mutable ZShmMut (ZShm -> ZShmMut)
        let owned_mut: Result<ZShmMut, _> = owned.try_into();
        // the attempt fails because ZShm has two existing references ('owned' and inside 'payload')
        assert!(owned_mut.is_err())
    }

    // branch to illustrate mutable access to SHM data
    {
        // deserialize ZBytes as mutably borrowed zshm (ZBytes -> &mut zshm)
        let borrowed_shm_buf = payload.as_shm_mut().unwrap();

        // convert zshm to zshmmut (&mut zshm -> &mut zshmmut)
        let _borrowed_shm_buf_mut: &mut zshmmut = borrowed_shm_buf.try_into().unwrap();
    }
}
