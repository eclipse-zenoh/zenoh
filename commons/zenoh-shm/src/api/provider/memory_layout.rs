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

use std::{fmt::Display, marker::PhantomData, num::NonZeroUsize};

use crate::api::provider::types::{AllocAlignment, ZLayoutError};

/// A trait for types that can represent an allocation layout to be used for buffer allocation requests.
#[zenoh_macros::unstable_doc]
pub trait BufferLayout {}
impl<T: MemLayout> BufferLayout for T {}
impl<T> BufferLayout for LayoutForType<T> {}

/// A trait for types that can represent a memory layout.
#[zenoh_macros::unstable_doc]
pub trait MemLayout {}
impl<T: TryIntoMemoryLayout> MemLayout for T {}
impl MemLayout for MemoryLayout {}
impl MemLayout for &MemoryLayout {}
impl<T> MemLayout for StaticLayout<T> {}

#[zenoh_macros::unstable_doc]
pub trait TryIntoMemoryLayout: TryInto<MemoryLayout, Error = ZLayoutError> {}
impl<T> TryIntoMemoryLayout for T where T: TryInto<MemoryLayout, Error = ZLayoutError> {}

/// Memory layout representation: alignment and size aligned for this alignment
#[zenoh_macros::unstable_doc]
#[derive(Debug, Clone)]
pub struct MemoryLayout {
    size: NonZeroUsize,
    alignment: AllocAlignment,
}

impl Display for MemoryLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[size={},alignment={}]",
            self.size, self.alignment
        ))
    }
}

impl MemoryLayout {
    /// Try to create a new memory layout.
    ///
    /// # Errors
    ///
    /// This function will return an error if zero size have passed or if the provided size is not the multiply of the alignment.
    #[zenoh_macros::unstable_doc]
    pub fn new<T>(size: T, alignment: AllocAlignment) -> Result<Self, ZLayoutError>
    where
        T: TryInto<NonZeroUsize>,
    {
        let Ok(size) = size.try_into() else {
            return Err(ZLayoutError::IncorrectLayoutArgs);
        };

        // size of a layout must be a multiple of its alignment!
        match size.get() % alignment.get_alignment_value() {
            0 => Ok(Self { size, alignment }),
            _ => Err(ZLayoutError::IncorrectLayoutArgs),
        }
    }

    /// #SAFETY: this is safe if size is a multiply of alignment
    /// Note: not intended for public APIs as it is really very unsafe
    unsafe fn new_unchecked(size: NonZeroUsize, alignment: AllocAlignment) -> Self {
        Self { size, alignment }
    }

    #[zenoh_macros::unstable_doc]
    pub fn size(&self) -> NonZeroUsize {
        self.size
    }

    #[zenoh_macros::unstable_doc]
    pub fn alignment(&self) -> AllocAlignment {
        self.alignment
    }

    /// Realign the layout for new alignment. The alignment must be >= of the existing one.
    /// # Examples
    ///
    /// ```
    /// use zenoh_shm::api::provider::types::AllocAlignment;
    /// use zenoh_shm::api::provider::memory_layout::MemoryLayout;
    ///
    /// // 8 bytes with 4-byte alignment
    /// let layout4b = MemoryLayout::new(8, AllocAlignment::new(2).unwrap()).unwrap();
    ///
    /// // Try to realign with 2-byte alignment
    /// let layout2b = layout4b.extend(AllocAlignment::new(1).unwrap());
    /// assert!(layout2b.is_err()); // fails because new alignment must be >= old
    ///
    /// // Try to realign with 8-byte alignment
    /// let layout8b = layout4b.extend(AllocAlignment::new(3).unwrap());
    /// assert!(layout8b.is_ok()); // ok
    /// ```
    #[zenoh_macros::unstable_doc]
    pub fn extend(&self, new_alignment: AllocAlignment) -> Result<MemoryLayout, ZLayoutError> {
        if self.alignment <= new_alignment {
            let new_size = new_alignment.align_size(self.size);
            return MemoryLayout::new(new_size, new_alignment);
        }
        Err(ZLayoutError::IncorrectLayoutArgs)
    }
}

impl TryFrom<NonZeroUsize> for MemoryLayout {
    type Error = ZLayoutError;

    fn try_from(value: NonZeroUsize) -> Result<Self, Self::Error> {
        MemoryLayout::new(value, AllocAlignment::ALIGN_1_BYTE)
    }
}

impl TryFrom<usize> for MemoryLayout {
    type Error = ZLayoutError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        MemoryLayout::new(value, AllocAlignment::ALIGN_1_BYTE)
    }
}

impl TryFrom<(NonZeroUsize, AllocAlignment)> for MemoryLayout {
    type Error = ZLayoutError;

    fn try_from(value: (NonZeroUsize, AllocAlignment)) -> Result<Self, Self::Error> {
        MemoryLayout::new(value.0, value.1)
    }
}

impl TryFrom<(usize, AllocAlignment)> for MemoryLayout {
    type Error = ZLayoutError;

    fn try_from(value: (usize, AllocAlignment)) -> Result<Self, Self::Error> {
        MemoryLayout::new(value.0, value.1)
    }
}

/// Helper type to build LayoutForType
#[zenoh_macros::unstable_doc]
pub struct BuildLayout;

impl BuildLayout {
    /// Create a new AllocAlignment for value type
    #[zenoh_macros::unstable_doc]
    pub fn for_val<T>(_: &T) -> LayoutForType<T> {
        Self::for_type::<T>()
    }

    /// Create a new AllocAlignment for type
    #[zenoh_macros::unstable_doc]
    pub fn for_type<T>() -> LayoutForType<T> {
        LayoutForType::<T> {
            inner: StaticLayout::<T> {
                _phantom: PhantomData,
            },
        }
    }
}

/// A statically-known layout with type information.
///
/// Used in context of typed operations.
///
/// Statically-known layouts are always correct, zero-sized & zero-cost.
#[zenoh_macros::unstable_doc]
pub struct LayoutForType<T> {
    inner: StaticLayout<T>,
}

impl<T> Clone for LayoutForType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for LayoutForType<T> {}

impl<T> LayoutForType<T> {
    pub fn layout(&self) -> StaticLayout<T> {
        self.inner
    }
}

impl<T> From<LayoutForType<T>> for MemoryLayout {
    fn from(value: LayoutForType<T>) -> Self {
        value.layout().into()
    }
}

/// A statically-known layout WITHOUT type information.
///
/// Statically-known layouts are always correct, zero-sized & zero-cost.
#[zenoh_macros::unstable_doc]
pub struct StaticLayout<T> {
    _phantom: PhantomData<T>,
}

impl<T> Clone for StaticLayout<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StaticLayout<T> {}

impl<T> StaticLayout<T> {
    pub const fn size(&self) -> NonZeroUsize {
        // SAFETY: this is safe because std::mem::size_of should always return >0 for T: Sized
        unsafe { NonZeroUsize::new_unchecked(std::mem::size_of::<T>()) }
    }

    pub const fn alignment(&self) -> AllocAlignment {
        AllocAlignment::for_type::<T>()
    }
}
impl<T> From<StaticLayout<T>> for MemoryLayout {
    fn from(value: StaticLayout<T>) -> Self {
        // SAFETY: this is safe as StaticLayout always gives correct layout arguments
        unsafe { MemoryLayout::new_unchecked(value.size(), value.alignment()) }
    }
}
