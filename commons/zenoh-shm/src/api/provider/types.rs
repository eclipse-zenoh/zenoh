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

use std::{fmt::Display, num::NonZeroUsize};

use super::chunk::AllocatedChunk;
use crate::api::buffer::zshmmut::ZShmMut;

/// Allocation errors
///
/// NeedDefragment: defragmentation needed
/// OutOfMemory: the provider is out of memory
/// Other: other error
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub enum ZAllocError {
    NeedDefragment,
    OutOfMemory,
    Other(zenoh_result::Error),
}

impl From<zenoh_result::Error> for ZAllocError {
    fn from(value: zenoh_result::Error) -> Self {
        Self::Other(value)
    }
}

/// alignment in powers of 2: 0 == 1-byte alignment, 1 == 2byte, 2 == 4byte, 3 == 8byte etc
#[zenoh_macros::unstable_doc]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AllocAlignment {
    pow: u8,
}

impl Display for AllocAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("[{}]", self.get_alignment_value()))
    }
}

impl Default for AllocAlignment {
    fn default() -> Self {
        Self {
            pow: (std::mem::align_of::<u32>() as f64).log2().round() as u8,
        }
    }
}

impl AllocAlignment {
    /// Try to create a new AllocAlignment from alignment representation in powers of 2.
    ///
    /// # Errors
    ///
    /// This function will return an error if provided alignment power cannot fit into usize.
    #[zenoh_macros::unstable_doc]
    pub const fn new(pow: u8) -> Result<Self, ZLayoutError> {
        match pow {
            pow if pow < usize::BITS as u8 => Ok(Self { pow }),
            _ => Err(ZLayoutError::IncorrectLayoutArgs),
        }
    }

    /// Get alignment in normal units (bytes)
    #[zenoh_macros::unstable_doc]
    pub fn get_alignment_value(&self) -> NonZeroUsize {
        // SAFETY: this is safe because we limit pow in new based on usize size
        unsafe { NonZeroUsize::new_unchecked(1usize << self.pow) }
    }

    /// Align size according to inner alignment.
    /// This call may extend the size (see the example)
    /// # Examples
    ///
    /// ```
    /// use zenoh_shm::api::provider::types::AllocAlignment;
    ///
    /// let alignment = AllocAlignment::new(2).unwrap(); // 4-byte alignment
    /// let initial_size = 7.try_into().unwrap();
    /// let aligned_size = alignment.align_size(initial_size);
    /// assert_eq!(aligned_size.get(), 8);
    /// ```
    #[zenoh_macros::unstable_doc]
    pub fn align_size(&self, size: NonZeroUsize) -> NonZeroUsize {
        let alignment = self.get_alignment_value();
        match size.get() % alignment {
            0 => size,
            // SAFETY:
            // This unsafe block is always safe:
            // 1. 0 < remainder < alignment
            // 2. because of 1, the value of (alignment.get() - remainder) is always > 0
            // 3. because of 2, we add nonzero size to nonzero (alignment.get() - remainder) and it is always positive if no overflow
            // 4. we make sure that there is no overflow condition in 3 by means of alignment limitation in `new` by limiting pow value
            remainder => unsafe {
                NonZeroUsize::new_unchecked(size.get() + (alignment.get() - remainder))
            },
        }
    }
}

/// Memory layout representation: alignment and size aligned for this alignment
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
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

        // size of an allocation must be a miltiple of it's alignment!
        match size.get() % alignment.get_alignment_value() {
            0 => Ok(Self { size, alignment }),
            _ => Err(ZLayoutError::IncorrectLayoutArgs),
        }
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
    /// use zenoh_shm::api::provider::types::MemoryLayout;
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

/// Layouting errors
///
/// IncorrectLayoutArgs: layout arguments are incorrect
/// ProviderIncompatibleLayout: layout incompatible with provider
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub enum ZLayoutError {
    IncorrectLayoutArgs,
    ProviderIncompatibleLayout,
}

/// SHM chunk allocation result
#[zenoh_macros::unstable_doc]
pub type ChunkAllocResult = Result<AllocatedChunk, ZAllocError>;

/// SHM buffer allocation result
#[zenoh_macros::unstable_doc]
pub type BufAllocResult = Result<ZShmMut, ZAllocError>;

/// Layouting and allocation errors
///
/// Alloc: allocation error
/// Layout: layouting error
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub enum ZLayoutAllocError {
    Alloc(ZAllocError),
    Layout(ZLayoutError),
}

/// SHM buffer layouting and allocation result
#[zenoh_macros::unstable_doc]
pub type BufLayoutAllocResult = Result<ZShmMut, ZLayoutAllocError>;
