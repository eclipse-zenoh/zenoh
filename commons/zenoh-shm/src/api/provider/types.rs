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

use std::fmt::Display;

use zenoh_result::{bail, ZResult};

use crate::api::slice::zsliceshmmut::ZSliceShmMut;

use super::chunk::AllocatedChunk;

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

/// alignemnt in powers of 2: 0 == 1-byte alignment, 1 == 2byte, 2 == 4byte, 3 == 8byte etc
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
    pub fn new(pow: u8) -> Self {
        Self { pow }
    }

    /// Get alignment in normal units (bytes)
    #[zenoh_macros::unstable_doc]
    pub fn get_alignment_value(&self) -> usize {
        1usize << self.pow
    }

    /// Align size according to inner alignment.
    /// This call may extend the size (see the example)
    /// # Examples
    ///
    /// ```
    /// use zenoh_shm::api::provider::types::AllocAlignment;
    ///
    /// let alignment = AllocAlignment::new(2); // 4-byte alignment
    /// let initial_size: usize = 7;
    /// let aligned_size = alignment.align_size(initial_size);
    /// assert_eq!(aligned_size, 8);
    /// ```
    #[zenoh_macros::unstable_doc]
    pub fn align_size(&self, size: usize) -> usize {
        let alignment = self.get_alignment_value();
        match size % alignment {
            0 => size,
            remainder => size + (alignment - remainder),
        }
    }
}

/// Memory layout representation: alignemnt and size aligned for this alignment
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct MemoryLayout {
    size: usize,
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
    /// Try to create a new memory layout
    #[zenoh_macros::unstable_doc]
    pub fn new(size: usize, alignment: AllocAlignment) -> ZResult<Self> {
        // size of an allocation must be a miltiple of it's alignment!
        match size % alignment.get_alignment_value() {
            0 => Ok(Self { size, alignment }),
            _ => bail!("size of an allocation must be a miltiple of it's alignment!"),
        }
    }

    #[zenoh_macros::unstable_doc]
    pub fn size(&self) -> usize {
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
    /// let layout4b = MemoryLayout::new(8, AllocAlignment::new(2)).unwrap();
    ///
    /// // Try to realign with 2-byte alignment
    /// let layout2b = layout4b.extend(AllocAlignment::new(1));
    /// assert!(layout2b.is_err()); // fails because new alignment must be >= old
    ///
    /// // Try to realign with 8-byte alignment
    /// let layout8b = layout4b.extend(AllocAlignment::new(3));
    /// assert!(layout8b.is_ok()); // ok
    /// ```
    #[zenoh_macros::unstable_doc]
    pub fn extend(&self, new_alignment: AllocAlignment) -> ZResult<MemoryLayout> {
        if self.alignment <= new_alignment {
            let new_size = new_alignment.align_size(self.size);
            return MemoryLayout::new(new_size, new_alignment);
        }
        bail!(
            "Cannot extend alignment form {} to {}: new alignment must be >= old!",
            self.alignment,
            new_alignment
        )
    }
}

/// SHM chunk allocation result
#[zenoh_macros::unstable_doc]
pub type ChunkAllocResult = Result<AllocatedChunk, ZAllocError>;

/// SHM buffer allocation result
#[zenoh_macros::unstable_doc]
pub type BufAllocResult = Result<ZSliceShmMut, ZAllocError>;
