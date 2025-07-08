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
    buffer::{traits::ShmConvert, zshmmut::ZShmMut},
    provider::shm_provider::ShmProviderBuilder,
};

#[repr(C)]
#[stabby::stabby]
struct SharedByteData {
    data: [u8; 64],
}

fn make_shm_buffer() -> ZShmMut<[u8]> {
    let provider = ShmProviderBuilder::default_backend(65536).wait().unwrap();
    provider.alloc(64).wait().unwrap()
}

fn validate_raw_buffer_consistency(buffer: &mut ZShmMut<[u8]>) {
    buffer.as_mut().fill(0);
}

fn validate_typed_buffer_consistency(buffer: &mut ZShmMut<SharedByteData>) {
    buffer.as_mut().data.fill(0);
}

#[test]
fn shm_buffer_morph() {
    let mut buffer = make_shm_buffer();
    validate_raw_buffer_consistency(&mut buffer);

    let mut buffer: ZShmMut<SharedByteData> = buffer.shm_convert().unwrap();
    validate_typed_buffer_consistency(&mut buffer);

    let mut buffer: ZShmMut<[u8]> = buffer.shm_convert().ok().unwrap();
    validate_raw_buffer_consistency(&mut buffer);
}
