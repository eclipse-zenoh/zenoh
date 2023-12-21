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
use zenoh_result::{bail, ZResult};

use super::segment::Segment;

pub struct ArrayInSHM<ID, Elem, ElemIndex> {
    inner: Segment<ID>,
    _phantom: PhantomData<(Elem, ElemIndex)>,
}

impl<ID, Elem, ElemIndex> ArrayInSHM<ID, Elem, ElemIndex>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
    ElemIndex: Unsigned + PrimInt + 'static + AsPrimitive<usize>,
    Elem: Sized,
    isize: AsPrimitive<ElemIndex>,
{
    pub fn create(elem_count: usize, file_prefix: &str) -> ZResult<Self> {
        let max: usize = ElemIndex::max_value().as_();
        if elem_count > max + 1 {
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
        self.inner.id.clone()
    }

    pub fn elem_count(&self) -> usize {
        self.inner.shmem.len() / size_of::<Elem>()
    }

    pub unsafe fn elem(&self, index: ElemIndex) -> *const Elem {
        (self.inner.shmem.as_ptr() as *const Elem).add(index.as_())
    }

    pub unsafe fn index(&self, elem: *const Elem) -> ElemIndex {
        elem.offset_from(self.inner.shmem.as_ptr() as *const Elem)
            .as_()
    }
}
