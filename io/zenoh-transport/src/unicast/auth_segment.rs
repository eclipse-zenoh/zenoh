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

use std::sync::atomic::{AtomicU64, Ordering};

use zenoh_result::ZResult;
use zenoh_shm::posix_shm::array::ArrayInSHM;

const AUTH_SEGMENT_PREFIX: &str = "auth";

pub(crate) type AuthSegmentID = u32;
pub(crate) type AuthChallenge = u64;

type InnerChallenge = AtomicU64;

#[derive(Debug)]
pub struct AuthSegment {
    array: ArrayInSHM<AuthSegmentID, InnerChallenge, usize>,
}

impl AuthSegment {
    pub fn create(challenge: AuthChallenge) -> ZResult<Self> {
        let array =
            ArrayInSHM::<AuthSegmentID, InnerChallenge, usize>::create(1, AUTH_SEGMENT_PREFIX)?;
        unsafe { (*array.elem(0)).store(challenge, Ordering::Relaxed) };
        Ok(Self { array })
    }

    pub fn open(id: AuthSegmentID) -> ZResult<Self> {
        let array = ArrayInSHM::open(id, AUTH_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }

    pub fn challenge(&self) -> AuthChallenge {
        unsafe { (*self.array.elem(0)).load(Ordering::Relaxed) }
    }

    pub fn id(&self) -> AuthSegmentID {
        self.array.id()
    }
}
