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

use rand::{Rng, SeedableRng};
use tokio::sync::RwLock;
use zenoh_core::zerror;
use zenoh_crypto::PseudoRng;
use zenoh_result::ZResult;
use zenoh_shm::{SharedMemoryBuf, SharedMemoryManager, SharedMemoryReader};

pub(crate) type Challenge = u64;
const NAME: &str = "zshm";

/*************************************/
/*          Authenticator            */
/*************************************/
pub(crate) struct SharedMemoryUnicast {
    // Rust guarantees that fields are dropped in the order of declaration.
    // Buffer needs to be dropped before the manager.
    pub(crate) challenge: SharedMemoryBuf,
    pub(crate) _manager: SharedMemoryManager,
    pub(crate) reader: RwLock<SharedMemoryReader>,
}

unsafe impl Sync for SharedMemoryUnicast {}

impl SharedMemoryUnicast {
    pub fn make() -> ZResult<SharedMemoryUnicast> {
        // Create a challenge for session establishment
        let mut prng = PseudoRng::from_entropy();
        let nonce = prng.gen::<Challenge>();
        let size = std::mem::size_of::<Challenge>();

        let mut _manager = SharedMemoryManager::make(format!("{NAME}.{nonce}"), size)?;

        let mut challenge = _manager.alloc(size).map_err(|e| zerror!("{e}"))?;
        let slice = unsafe { challenge.as_mut_slice() };
        slice[0..size].copy_from_slice(&nonce.to_le_bytes());

        let shmauth = SharedMemoryUnicast {
            challenge,
            _manager,
            reader: RwLock::new(SharedMemoryReader::new()),
        };
        Ok(shmauth)
    }
}
