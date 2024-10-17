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

use std::fmt::{Debug, Display};

use rand::Rng;
use shared_memory::{Shmem, ShmemConf, ShmemError};
use zenoh_result::{bail, zerror, ZResult};

use crate::cleanup::CLEANUP;

#[cfg(target_os = "linux")]
use super::segment_lock::unix::{ExclusiveShmLock, ShmLock};

const SEGMENT_DEDICATE_TRIES: usize = 100;
const ECMA: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_ECMA_182);

/// Segment of shared memory identified by an ID
pub struct Segment<ID>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    shmem: Shmem, // <-------------|
    id: ID,       //               |
    #[cfg(target_os = "linux")] // | location of these two fields matters!
    _lock: Option<ShmLock>, // <---|
}

#[cfg(target_os = "linux")]
impl<ID> Drop for Segment<ID>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    fn drop(&mut self) {
        if let Some(lock) = self._lock.take() {
            if let Ok(_exclusive) = std::convert::TryInto::<ExclusiveShmLock>::try_into(lock) {
                // in case if we are the last holder of this segment make ourselves owner to unlink it
                self.shmem.set_owner(true);
            }
        }
    }
}

impl<ID> Debug for Segment<ID>
where
    ID: Debug,
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segment")
            .field("shmem", &self.shmem.as_ptr())
            .field("id", &self.id)
            .finish()
    }
}

impl<ID> Segment<ID>
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    // Automatically generate free id and create a new segment identified by this id
    pub fn create(alloc_size: usize, id_prefix: &str) -> ZResult<Self> {
        for _ in 0..SEGMENT_DEDICATE_TRIES {
            // Generate random id
            let id: ID = rand::thread_rng().gen();
            let os_id = Self::os_id(id.clone(), id_prefix);

            #[cfg(target_os = "linux")]
            // Create lock to indicate that segment is managed
            let lock = {
                match ShmLock::create(&os_id) {
                    Ok(lock) => lock,
                    Err(_) => continue,
                }
            };

            // Register cleanup routine to make sure Segment will be unlinked on exit
            let c_os_id = os_id.clone();
            CLEANUP.read().register_cleanup(Box::new(move || {
                if let Ok(mut shmem) = ShmemConf::new().os_id(c_os_id).open() {
                    shmem.set_owner(true);
                    drop(shmem);
                }
            }));

            // Try to create a new segment identified by prefix and generated id.
            // If creation fails because segment already exists for this id,
            // the creation attempt will be repeated with another id
            match ShmemConf::new().size(alloc_size).os_id(os_id).create() {
                Ok(shmem) => {
                    tracing::debug!(
                        "Created SHM segment, size: {alloc_size}, prefix: {id_prefix}, id: {id}"
                    );
                    #[cfg(target_os = "linux")]
                    let shmem = {
                        let mut shmem = shmem;
                        shmem.set_owner(false);
                        shmem
                    };
                    return Ok(Segment {
                        shmem,
                        id,
                        #[cfg(target_os = "linux")]
                        _lock: Some(lock),
                    });
                }
                Err(ShmemError::LinkExists) => {}
                Err(ShmemError::MappingIdExists) => {}
                Err(e) => bail!("Unable to create POSIX shm segment: {}", e),
            }
        }
        bail!("Unable to dedicate POSIX shm segment file after {SEGMENT_DEDICATE_TRIES} tries!");
    }

    // Open an existing segment identified by id
    pub fn open(id: ID, id_prefix: &str) -> ZResult<Self> {
        let os_id = Self::os_id(id.clone(), id_prefix);

        #[cfg(target_os = "linux")]
        // Open lock to indicate that segment is managed
        let lock = ShmLock::open(&os_id)?;

        // Open SHM segment
        let shmem = ShmemConf::new().os_id(os_id).open().map_err(|e| {
            zerror!(
                "Error opening POSIX shm segment id {id}, prefix: {id_prefix}: {}",
                e
            )
        })?;

        tracing::debug!("Opened SHM segment, prefix: {id_prefix}, id: {id}");

        Ok(Self {
            shmem,
            id,
            #[cfg(target_os = "linux")]
            _lock: Some(lock),
        })
    }

    fn os_id(id: ID, id_prefix: &str) -> String {
        let os_id_str = format!("{id_prefix}_{id}");
        let crc_os_id_str = ECMA.checksum(os_id_str.as_bytes());
        format!("{:x}.zenoh", crc_os_id_str)
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.shmem.as_ptr()
    }

    /// Returns the length of this [`Segment<ID>`].
    /// NOTE: one some platforms (at least windows) the returned len will be the actual length of an shm segment
    /// (a required len rounded up to the nearest multiply of page size), on other (at least linux and macos) this
    /// returns a value requested upon segment creation
    pub fn len(&self) -> usize {
        self.shmem.len()
    }

    // TODO: dead code warning occurs because of `tested_crate_module!()` macro when feature `test` is not enabled. Better to fix that
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn id(&self) -> ID {
        self.id.clone()
    }
}
