//
// Copyright (c) 2024 ZettaScale Technology
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

use crate::{
    api::client_storage::GLOBAL_CLIENT_STORAGE,
    cleanup::CLEANUP,
    metadata::{storage::GLOBAL_METADATA_STORAGE, subscription::GLOBAL_METADATA_SUBSCRIPTION},
    watchdog::{confirmator::GLOBAL_CONFIRMATOR, validator::GLOBAL_VALIDATOR},
};

pub fn init() {
    let _ = CLEANUP.write();
    let _ = GLOBAL_CLIENT_STORAGE.write();
    let _ = GLOBAL_METADATA_STORAGE.write();
    let _ = GLOBAL_METADATA_SUBSCRIPTION.write();
    let _ = GLOBAL_CONFIRMATOR.write();
    let _ = GLOBAL_VALIDATOR.write();
}
