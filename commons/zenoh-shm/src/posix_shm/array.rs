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

use std::{fmt::Display, marker::PhantomData, mem::size_of};

use num_traits::{AsPrimitive, PrimInt, Unsigned};
use stabby::IStable;
use zenoh_result::{bail, ZResult};

use super::segment::Segment;

/// An SHM segment that is intended to be an array of elements of some certain type
#[derive(Debug)]
pub struct ArrayInSHM<ID, Elem, ElemIndex>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    inner: Segment<ID>,
    _phantom: PhantomData<(Elem, ElemIndex)>,
}

unsafe impl<ID, Elem: Sync, ElemIndex> Sync for ArrayInSHM<ID, Elem, ElemIndex>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
}
unsafe impl<ID, Elem: Send, ElemIndex> Send for ArrayInSHM<ID, Elem, ElemIndex>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
}

impl<ID, Elem, ElemIndex> ArrayInSHM<ID, Elem, ElemIndex>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
    ElemIndex: Unsigned + PrimInt + 'static + AsPrimitive<usize>,
    Elem: IStable<ContainsIndirections = stabby::abi::B0>,
    isize: AsPrimitive<ElemIndex>,
{
    // Perform compile time check that Elem is not a ZST in such a way `elem_count` can not panic.
    const _S: () = if size_of::<Elem>() == 0 {
        panic!("Elem is a ZST. ZSTs are not allowed as ArrayInSHM generic");
    };

    pub fn create(elem_count: usize, file_prefix: &str) -> ZResult<Self> {
        if elem_count == 0 {
            bail!("Unable to create SHM array segment of 0 elements")
        }

        let max: usize = ElemIndex::max_value().as_();
        if elem_count - 1 > max {
            bail!("Unable to create SHM array segment of {elem_count} elements: out of range for ElemIndex!")
        }

        let alloc_size = elem_count * size_of::<Elem>();
        let inner = Segment::create(alloc_size, file_prefix)?;
        Ok(Self {
            inner,
            _phantom: PhantomData,
        })
    }

    pub fn open(id: ID, file_prefix: &str) -> ZResult<Self> {
        let inner = Segment::open(id, file_prefix)?;
        Ok(Self {
            inner,
            _phantom: PhantomData,
        })
    }

    pub fn id(&self) -> ID {
        self.inner.id()
    }

    pub fn elem_count(&self) -> usize {
        self.inner.len() / size_of::<Elem>()
    }

    /// # Safety
    /// Retrieves const element by it's index. This is safe if the index doesn't go out of underlying array.
    /// Additional assert to check the index validity is added for "test" feature
    pub unsafe fn elem(&self, index: ElemIndex) -> *const Elem {
        #[cfg(feature = "test")]
        assert!(self.inner.len() > index.as_() * size_of::<Elem>());
        (self.inner.as_ptr() as *const Elem).add(index.as_())
    }

    /// # Safety
    /// Retrieves mut element by it's index. This is safe if the index doesn't go out of underlying array.
    /// Additional assert to check the index validity is added for "test" feature
    pub unsafe fn elem_mut(&self, index: ElemIndex) -> *mut Elem {
        #[cfg(feature = "test")]
        assert!(self.inner.len() > index.as_() * size_of::<Elem>());
        (self.inner.as_ptr() as *mut Elem).add(index.as_())
    }

    /// # Safety
    /// Calculates element's index. This is safe if the element belongs to underlying array.
    /// Additional assert is added for "test" feature
    pub unsafe fn index(&self, elem: *const Elem) -> ElemIndex {
        let index = elem.offset_from(self.inner.as_ptr() as *const Elem);
        #[cfg(feature = "test")]
        {
            assert!(index >= 0);
            assert!(self.inner.len() > index as usize * size_of::<Elem>());
        }
        index.as_()
    }
}
