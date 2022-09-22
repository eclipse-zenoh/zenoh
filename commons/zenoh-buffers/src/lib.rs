//
// Copyright (c) 2022 ZettaScale Technology
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

//! Provide [ZBuf] and [WBuf] as convenient buffers used for serialization and deserialization.
pub mod traits;
pub use traits::*;

mod zslice;
pub use zslice::*;

mod zbuf;
pub use zbuf::*;

mod wbuf;
pub use wbuf::*;

#[cfg(feature = "shared-memory")]
mod shm;
#[cfg(feature = "shared-memory")]
pub use shm::*;
