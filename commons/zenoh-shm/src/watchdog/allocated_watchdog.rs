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

use super::{descriptor::OwnedDescriptor, storage::GLOBAL_STORAGE, validator::GLOBAL_VALIDATOR};

#[derive(Debug)]
pub struct AllocatedWatchdog {
    pub descriptor: OwnedDescriptor,
}

impl AllocatedWatchdog {
    pub(crate) fn new(descriptor: OwnedDescriptor) -> Self {
        // reset descriptor on allocation
        descriptor.validate();
        Self { descriptor }
    }
}

impl Drop for AllocatedWatchdog {
    fn drop(&mut self) {
        GLOBAL_VALIDATOR.read().remove(self.descriptor.clone());
        GLOBAL_STORAGE.read().free_watchdog(self.descriptor.clone());
    }
}
