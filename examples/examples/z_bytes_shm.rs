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
    bytes::ZBytes,
    shm::{zshmmut, ShmProviderBuilder, ZShm, ZShmMut},
    Wait,
};

fn main() {
    // Create SHM provider with default backend
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let provider = ShmProviderBuilder::default_backend(4096).wait().unwrap();

    // Allocate an SHM buffer
    // NOTE: For allocation API please check z_alloc_shm.rs example
    let mut owned_shm_buf_mut = provider.alloc(1024).wait().unwrap();

    // mutable and immutable API
    let _data: &[u8] = &owned_shm_buf_mut;
    let _data_mut: &mut [u8] = &mut owned_shm_buf_mut;

    // convert into immutable owned buffer (ZShmMut -> ZShm)
    let owned_shm_buf: ZShm = owned_shm_buf_mut.into();

    // immutable API
    let _data: &[u8] = &owned_shm_buf;

    // convert again into mutable owned buffer (ZShm -> ZShmMut)
    let mut owned_shm_buf_mut: ZShmMut = owned_shm_buf.try_into().unwrap();

    // mutable and immutable API
    let _data: &[u8] = &owned_shm_buf_mut;
    let _data_mut: &mut [u8] = &mut owned_shm_buf_mut;

    // build a ZBytes from an SHM buffer (ZShmMut -> ZBytes)
    let mut payload: ZBytes = owned_shm_buf_mut.into();

    // branch to illustrate immutable access to SHM data
    {
        // deserialize ZBytes as an immutably borrowed zshm (ZBytes -> &zshm)
        let borrowed_shm_buf = payload.as_shm().unwrap();

        // immutable API
        let _data: &[u8] = borrowed_shm_buf;

        // construct owned buffer from borrowed type (&zshm -> ZShm)
        let owned = borrowed_shm_buf.to_owned();

        // immutable API
        let _data: &[u8] = &owned;

        // try to construct mutable ZShmMut (ZShm -> ZShmMut)
        let owned_mut: Result<ZShmMut, _> = owned.try_into();
        // the attempt fails because ZShm has two existing references ('owned' and inside 'payload')
        assert!(owned_mut.is_err())
    }

    // branch to illustrate mutable access to SHM data
    {
        // deserialize ZBytes as mutably borrowed zshm (ZBytes -> &mut zshm)
        let borrowed_shm_buf = payload.as_shm_mut().unwrap();

        // immutable API
        let _data: &[u8] = borrowed_shm_buf;

        // convert zshm to zshmmut (&mut zshm -> &mut zshmmut)
        let borrowed_shm_buf_mut: &mut zshmmut = borrowed_shm_buf.try_into().unwrap();

        // mutable and immutable API
        let _data: &[u8] = borrowed_shm_buf_mut;
        let _data_mut: &mut [u8] = borrowed_shm_buf_mut;
    }
}
