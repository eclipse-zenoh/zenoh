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

use crate::ZSlice;
use core::ops::{Deref, DerefMut};

/*************************************/
/*              BUILDER              */
/*************************************/
pub struct ZSliceMutBuilder<'a> {
    slice: &'a mut ZSlice,
}

impl<'a> ZSliceMutBuilder<'a> {
    pub(crate) fn new(slice: &'a mut ZSlice) -> Self {
        Self { slice }
    }

    /// Check if ZSlice shares the underlying data.
    /// Will return current ZSlice state along with proper resolving builder
    pub fn is_sharing(self) -> SharingResult<'a> {
        match self.slice.is_unique() {
            true => SharingResult::Unique(ZSliceMutBuilderUncheckedSafe::new(self.slice)),
            false => SharingResult::Sharing(ZSliceMutBuilderCopy::new(self.slice)),
        }
    }

    /// Perform implicit copy if ZSlice shares the underlying data
    pub fn copy_if_sharing(self) -> ZSliceMutBuilderCopyIfShared<'a> {
        ZSliceMutBuilderCopyIfShared::new(self.slice)
    }

    /// Perform implicit copy anyway
    pub fn copy(self) -> ZSliceMutBuilderCopy<'a> {
        ZSliceMutBuilderCopy::new(self.slice)
    }

    /// Fail and return None if ZSlice shares the underlying data
    pub fn fail_if_sharing(self) -> ZSliceMutBuilderFail<'a> {
        ZSliceMutBuilderFail::new(self.slice)
    }

    /// # Safety
    /// Unconditionally borrows ZSlice's data as mutable without runtime checks.
    /// This is safe if all of the following conditions are met:
    /// - ZSlice instance uniquely references the underlying data
    /// - shared memory reference for underlying data is unique (for shared-memory buffers)
    /// This method is mostly usable for mutating recently-allocated shared memory buffers
    pub unsafe fn unchecked(self) -> ZSliceMutBuilderUncheckedUnsafe<'a> {
        ZSliceMutBuilderUncheckedUnsafe::new(self.slice)
    }
}

pub enum SharingResult<'a> {
    Sharing(ZSliceMutBuilderCopy<'a>),
    Unique(ZSliceMutBuilderUncheckedSafe<'a>),
}

pub struct ZSliceMutBuilderUncheckedSafe<'a> {
    inner: ZSliceMutBuilderUncheckedUnsafe<'a>,
}

impl<'a> ZSliceMutBuilderUncheckedSafe<'a> {
    fn new(slice: &'a mut ZSlice) -> Self {
        Self {
            inner: ZSliceMutBuilderUncheckedUnsafe::new(slice),
        }
    }

    pub fn res(self) -> ZSliceMut<'a> {
        unsafe { self.inner.res() }
    }
}

pub struct ZSliceMutBuilderUncheckedUnsafe<'a> {
    slice: &'a mut ZSlice,
}

impl<'a> ZSliceMutBuilderUncheckedUnsafe<'a> {
    fn new(slice: &'a mut ZSlice) -> Self {
        Self { slice }
    }

    /// # Safety
    /// Unconditionally borrows ZSlice's data as mutable without runtime checks.
    /// This is safe if all of the following conditions are met:
    /// - ZSlice instance is unique
    /// - shared memory reference for underlying data is unique (for shared-memory buffers)
    /// This method is mostly usable for mutating recently-allocated shared memory buffers
    pub unsafe fn res(self) -> ZSliceMut<'a> {
        ZSliceMut::new(self.slice.as_slice_mut_unchecked())
    }
}

pub struct ZSliceMutBuilderCopy<'a> {
    slice: &'a mut ZSlice,
}

impl<'a> ZSliceMutBuilderCopy<'a> {
    fn new(slice: &'a mut ZSlice) -> Self {
        Self { slice }
    }

    pub fn res(self) -> ZSliceMut<'a> {
        ZSliceMut::new(self.slice.copy_as_slice_mut())
    }
}

pub struct ZSliceMutBuilderCopyIfShared<'a> {
    slice: &'a mut ZSlice,
}

impl<'a> ZSliceMutBuilderCopyIfShared<'a> {
    fn new(slice: &'a mut ZSlice) -> Self {
        Self { slice }
    }

    pub fn res(self) -> ZSliceMut<'a> {
        ZSliceMut::new(self.slice.as_slice_mut())
    }
}

pub struct ZSliceMutBuilderFail<'a> {
    slice: &'a mut ZSlice,
}

impl<'a> ZSliceMutBuilderFail<'a> {
    fn new(slice: &'a mut ZSlice) -> Self {
        Self { slice }
    }

    pub fn res(self) -> Option<ZSliceMut<'a>> {
        self.slice.try_as_slice_mut().map(ZSliceMut::new)
    }
}

/*************************************/
/*             ZSLICEMUT             */
/*************************************/
pub struct ZSliceMut<'a> {
    slice: &'a mut [u8],
}

impl<'a> ZSliceMut<'a> {
    fn new(slice: &'a mut [u8]) -> Self {
        Self { slice }
    }
}

impl<'a> Drop for ZSliceMut<'a> {
    fn drop(&mut self) {}
}

impl<'a> Deref for ZSliceMut<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.slice
    }
}

impl<'a> DerefMut for ZSliceMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.slice
    }
}

impl<'a> AsRef<[u8]> for ZSliceMut<'a> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<'a> AsMut<[u8]> for ZSliceMut<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}
