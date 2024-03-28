//
// Copyright (c) 2022 ZettaScale Technology
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
use rand::{Rng, SeedableRng};
use tokio::sync::RwLock;
use zenoh_crypto::PseudoRng;
use zenoh_result::ZResult;
use zenoh_shm::{SharedMemoryManager, SharedMemoryReader};

pub(crate) type Challenge = u64;
const NAME: &str = "zshm_mcast";

pub(crate) struct SharedMemoryMulticast {
    pub(crate) _manager: SharedMemoryManager,
    pub(crate) reader: RwLock<SharedMemoryReader>,
}

unsafe impl Sync for SharedMemoryMulticast {}

impl SharedMemoryMulticast {
    pub fn make() -> ZResult<SharedMemoryMulticast> {
        let mut prng = PseudoRng::from_entropy();
        let nonce = prng.gen::<Challenge>();
        let size = std::mem::size_of::<Challenge>();

        let mut _manager = SharedMemoryManager::make(format!("{NAME}.{nonce}"), size)?;

        let shmauth = SharedMemoryMulticast {
            _manager,
            reader: RwLock::new(SharedMemoryReader::new()),
        };
        Ok(shmauth)
    }
}
