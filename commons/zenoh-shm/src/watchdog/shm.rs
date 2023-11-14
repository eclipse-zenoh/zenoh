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

use std::mem::size_of;

use rand::Rng;
use shared_memory::{Shmem, ShmemConf, ShmemError};
use zenoh_result::{bail, zerror, ZResult};

use super::descriptor::SegmentID;

const SEGMENT_DEDICATE_TRIES: usize = 100;

pub struct Segment {
    pub shmem: Shmem,
    pub id: SegmentID,
}

unsafe impl Send for Segment {}
unsafe impl Sync for Segment {}

impl Segment {
    pub fn create(watchdog_count: usize) -> ZResult<Self> {
        let alloc_size = (watchdog_count + 63) / 64 * size_of::<u64>();

        for _ in 0..SEGMENT_DEDICATE_TRIES {
            let id: SegmentID = rand::thread_rng().gen();

            match ShmemConf::new()
                .size(alloc_size)
                .flink(format!("watchdog_{id}"))
                .create()
            {
                Ok(shmem) => return Ok(Segment { shmem, id }),
                Err(ShmemError::LinkExists) => {}
                Err(e) => bail!("Unable to create watchdog shm segment: {}", e),
            }
        }
        bail!("Unable to dedicate watchdog shm segment file!");
    }

    pub fn open(id: SegmentID) -> ZResult<Self> {
        let shmem = ShmemConf::new()
            .flink(format!("watchdog_{id}"))
            .open()
            .map_err(|e| zerror!("Unable to open watchdog shm segment: {}", e))?;
        Ok(Self { shmem, id })
    }
}
