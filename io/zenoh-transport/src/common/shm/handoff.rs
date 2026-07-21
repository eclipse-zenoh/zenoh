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

use std::ops::{Index, IndexMut};

use zenoh_protocol::core::Priority;

#[derive(Debug)]
pub struct PriorityContainer<T: Sized> {
    per_prio_objects: [T; Priority::NUM],
}

impl<T: Sized> PriorityContainer<T> {
    pub fn new(per_prio_objects: [T; Priority::NUM]) -> Self {
        Self { per_prio_objects }
    }

    pub fn from_fn<E>(mut ctor_fn: impl FnMut(Priority) -> Result<T, E>) -> Result<Self, E> {
        // TODO: std::array::try_from_fn is unstable yet...
        let per_prio_objects = [
            ctor_fn(Priority::Control)?,
            ctor_fn(Priority::RealTime)?,
            ctor_fn(Priority::InteractiveHigh)?,
            ctor_fn(Priority::InteractiveLow)?,
            ctor_fn(Priority::DataHigh)?,
            ctor_fn(Priority::Data)?,
            ctor_fn(Priority::DataLow)?,
            ctor_fn(Priority::Background)?,
        ];
        Ok(Self { per_prio_objects })
    }

    pub fn from_fn_infallible(ctor_fn: impl Fn(Priority) -> T) -> Self {
        let per_prio_objects = std::array::from_fn(|prio| {
            // SAFETY: `Priority` is guaranteed to be in the range 0..=7, so this conversion is safe.
            ctor_fn(unsafe { (prio as u8).try_into().unwrap_unchecked() })
        });
        Self { per_prio_objects }
    }

    pub fn map<Tother: Sized>(self, map_fn: impl FnMut(T) -> Tother) -> PriorityContainer<Tother> {
        PriorityContainer::new(self.per_prio_objects.map(map_fn))
    }

    pub fn map_ref<Tother: Sized>(
        &self,
        map_fn: impl Fn(&T) -> Tother,
    ) -> PriorityContainer<Tother> {
        PriorityContainer::from_fn_infallible(|prio| {
            let obj = &self.per_prio_objects[prio as usize];
            map_fn(obj)
        })
    }
}

impl<'a, T: Sized> IntoIterator for &'a PriorityContainer<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        (&self.per_prio_objects).iter()
    }
}

impl<T: Sized> Index<Priority> for PriorityContainer<T> {
    type Output = T;

    fn index(&self, index: Priority) -> &Self::Output {
        &self.per_prio_objects[index as usize]
    }
}

impl<T: Sized> IndexMut<Priority> for PriorityContainer<T> {
    fn index_mut(&mut self, index: Priority) -> &mut Self::Output {
        &mut self.per_prio_objects[index as usize]
    }
}

impl<T: Sized + Clone> Clone for PriorityContainer<T> {
    fn clone(&self) -> Self {
        Self {
            per_prio_objects: self.per_prio_objects.clone(),
        }
    }
}
