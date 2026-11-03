//
// Copyright (c) 2026 ZettaScale Technology
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

use crate::types::BufferGroupId;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexGeneration(NonZeroU64);

impl IndexGeneration {
    pub const INVALID_MIN: u64 = 0;
    pub const INVALID_MAX: u64 = u64::MAX;
    /// Set in the `user_data` of the `RemoveBuffers` that retires the buffer
    /// group of a stopped task. Slot indices stay far below this bit: a slot
    /// is only allocated together with a `u16` buffer group id.
    const RETIRE_TAG: u64 = 1 << 31;
    /// Set in the `user_data` of the cancellation of a stopped task's
    /// receive.
    const CANCEL_TAG: u64 = 1 << 30;
    /// Set in the `user_data` of a `ProvideBuffers`, next to the buffer group
    /// id. The generation half stays zero, which no task index has.
    const PROVIDE_TAG: u64 = 1 << 29;

    /// `user_data` of the buffer-group retirement of this task.
    #[inline]
    pub const fn retire_user_data(self) -> u64 {
        self.0.get() | Self::RETIRE_TAG
    }

    #[inline]
    pub const fn is_retire_user_data(val: u64) -> bool {
        val & Self::RETIRE_TAG != 0 && val != Self::INVALID_MAX
    }

    /// `user_data` of the cancellation of this task's receive.
    #[inline]
    pub const fn cancel_user_data(self) -> u64 {
        self.0.get() | Self::CANCEL_TAG
    }

    #[inline]
    pub const fn is_cancel_user_data(val: u64) -> bool {
        val & (Self::CANCEL_TAG | Self::RETIRE_TAG) == Self::CANCEL_TAG && val >> 32 != 0
    }

    /// The task whose receive a cancellation targeted.
    ///
    /// `val` must satisfy [`Self::is_cancel_user_data`].
    #[inline]
    pub const fn cancel_target(val: u64) -> Self {
        // SAFETY: the generation half of a cancellation's user_data is non-zero
        unsafe { Self::new_unchecked(val & !Self::CANCEL_TAG) }
    }

    /// `user_data` of the buffers provided to `group`.
    #[inline]
    pub const fn provide_user_data(group: BufferGroupId) -> u64 {
        Self::PROVIDE_TAG | group as u64
    }

    #[inline]
    pub const fn is_provide_user_data(val: u64) -> bool {
        val >> 32 == 0 && val & Self::PROVIDE_TAG != 0
    }

    /// The buffer group a `ProvideBuffers` targeted.
    #[inline]
    pub const fn provided_group(val: u64) -> BufferGroupId {
        val as BufferGroupId
    }

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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::IndexGeneration;

    fn tasks() -> impl Iterator<Item = IndexGeneration> {
        [0, 1, 0xFFFF, 0x1_0000].into_iter().flat_map(|index| {
            [1, 2, u32::MAX]
                .into_iter()
                .map(move |g| IndexGeneration::new(index, NonZeroU32::new(g).unwrap()))
        })
    }

    /// Each kind of `user_data` the reader issues is recognized as itself
    /// only, and the tagged ones lead back to their task.
    #[test]
    fn user_data_tags_are_disjoint() {
        for task in tasks() {
            let task_ud: u64 = task.into();
            let retire = task.retire_user_data();
            let cancel = task.cancel_user_data();
            for (ud, is_retire, is_cancel) in [
                (task_ud, false, false),
                (retire, true, false),
                (cancel, false, true),
            ] {
                assert_eq!(IndexGeneration::is_retire_user_data(ud), is_retire);
                assert_eq!(IndexGeneration::is_cancel_user_data(ud), is_cancel);
                assert!(!IndexGeneration::is_provide_user_data(ud));
            }
            assert_eq!(IndexGeneration::cancel_target(cancel), task);
        }
        for group in [0, 1, u16::MAX] {
            let ud = IndexGeneration::provide_user_data(group);
            assert!(IndexGeneration::is_provide_user_data(ud));
            assert!(!IndexGeneration::is_retire_user_data(ud));
            assert!(!IndexGeneration::is_cancel_user_data(ud));
            assert_eq!(IndexGeneration::provided_group(ud), group);
        }
        for ud in [IndexGeneration::INVALID_MIN, IndexGeneration::INVALID_MAX] {
            assert!(!IndexGeneration::is_retire_user_data(ud));
            assert!(!IndexGeneration::is_cancel_user_data(ud));
            assert!(!IndexGeneration::is_provide_user_data(ud));
        }
    }
}
