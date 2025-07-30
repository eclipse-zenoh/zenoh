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
    buffer::{typed::Typed, zshmmut::ZShmMut},
    provider::shm_provider::ShmProviderBuilder,
};

#[repr(C)]
#[derive(zerocopy::KnownLayout, zerocopy::FromBytes)]
struct SharedByteData {
    data: [u8; 64],
}

fn make_shm_buffer() -> ZShmMut {
    let provider = ShmProviderBuilder::default_backend(65536).wait().unwrap();
    provider.alloc(64).wait().unwrap()
}

fn validate_raw_buffer_consistency(buffer: &mut ZShmMut) {
    buffer.as_mut().fill(0);
}

fn validate_typed_buffer_consistency(buffer: &mut impl AsMut<SharedByteData>) {
    buffer.as_mut().data.fill(0);
}

#[test]
fn shm_buffer_morph() {
    let mut buffer = make_shm_buffer();
    validate_raw_buffer_consistency(&mut buffer);

    let mut buffer: Typed<SharedByteData, _> = buffer.try_into().unwrap();
    validate_typed_buffer_consistency(&mut buffer);

    let mut buffer = buffer.into_inner();
    validate_raw_buffer_consistency(&mut buffer);
}
