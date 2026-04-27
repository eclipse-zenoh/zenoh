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

use std::num::{NonZeroU32, NonZeroU64};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexGeneration(NonZeroU64);

impl IndexGeneration {
    pub const INVALID_MIN: u64 = 0;
    pub const INVALID_MAX: u64 = u64::MAX;

    #[inline]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        // SAFETY: this is safe because generation is NonZeroU32
        Self(unsafe {
            NonZeroU64::new_unchecked(((generation.get() as u64) << 32) | (index as u64))
        })
    }

    /// # Safety
    ///
    /// This is safe if `val` is not INVALID_MIN and not INVALID_MAX.
    #[inline]
    pub const unsafe fn new_unchecked(val: u64) -> Self {
        Self(NonZeroU64::new_unchecked(val))
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.0.get() as u32
    }

    #[inline]
    pub const fn generation(self) -> NonZeroU32 {
        unsafe { NonZeroU32::new_unchecked((self.0.get() >> 32) as u32) }
    }
}

impl From<IndexGeneration> for u64 {
    #[inline]
    fn from(value: IndexGeneration) -> Self {
        value.0.get()
    }
}
