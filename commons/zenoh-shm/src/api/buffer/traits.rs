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

use std::ops::{Deref, DerefMut};

#[zenoh_macros::unstable_doc]
pub trait ShmBuf: Deref<Target = [u8]> + AsRef<[u8]> {
    #[zenoh_macros::unstable_doc]
    fn is_valid(&self) -> bool;
}

#[zenoh_macros::unstable_doc]
pub trait ShmBufMut: ShmBuf + DerefMut + AsMut<[u8]> {}
