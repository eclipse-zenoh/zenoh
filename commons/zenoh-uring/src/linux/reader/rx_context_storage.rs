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

use std::{num::NonZeroU32, rc::Rc};

use crate::reader::{index::IndexGeneration, rx_context::Rx};

#[derive(Debug)]
pub(crate) struct RxContextCell {
    context: Option<Rc<Rx>>,
    generation: NonZeroU32,
}

impl RxContextCell {
    fn new(context: Rc<Rx>, generation: NonZeroU32) -> Self {
        Self {
            context: Some(context),
            generation,
        }
    }

    fn get(&self, generation: NonZeroU32) -> Option<&Rx> {
        if self.generation == generation {
            return self.context.as_deref();
        }
        None
    }

    fn free(&mut self, generation: NonZeroU32) {
        if self.generation == generation {
            if let Some(context) = self.context.take() {
                tracing::debug!("Begin RxContext destroy {:?}", context);
                assert!(Rc::strong_count(&context) == 1);
                drop(context);
                tracing::debug!("End RxContext destroy!");
            }
        } else {
            tracing::debug!(
                "Unable to free: generation mismatch! {:?}, generation: {generation}",
                self
            );
        }
    }

    fn try_alloc(&mut self, generation: NonZeroU32, context: &Rc<Rx>) -> bool {
        if self.context.is_none() {
            self.context = Some(context.clone());
            self.generation = generation;
            return true;
        }
        false
    }
}

pub(crate) struct RxContextStorage {
    data: Vec<RxContextCell>,
    next_generation: NonZeroU32,
}

impl RxContextStorage {
    pub(crate) fn new() -> Self {
        let data = Vec::with_capacity(16);
        Self {
            data,
            next_generation: NonZeroU32::MIN,
        }
    }

    pub(crate) fn get(&self, id: IndexGeneration) -> Option<&Rx> {
        self.data[id.index() as usize].get(id.generation())
    }

    pub(crate) fn free(&mut self, id: IndexGeneration) {
        self.data[id.index() as usize].free(id.generation())
    }

    pub(crate) fn alloc(&mut self, context: Rc<Rx>) -> IndexGeneration {
        self.next_generation = self.next_generation.saturating_add(1);
        if self.next_generation == NonZeroU32::MAX {
            self.next_generation = NonZeroU32::MIN;
        }

        for (index, cell) in self.data.iter_mut().enumerate() {
            if cell.try_alloc(self.next_generation, &context) {
                return IndexGeneration::new(index as u32, self.next_generation);
            }
        }

        let new_cell = RxContextCell::new(context, self.next_generation);
        self.data.push(new_cell);
        IndexGeneration::new((self.data.len() - 1) as u32, self.next_generation)
    }
}
