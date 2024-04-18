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

use zenoh_result::ZResult;
use zenoh_shm::{api::common::types::ProtocolID, posix_shm::array::ArrayInSHM};

const AUTH_SEGMENT_PREFIX: &str = "auth";

pub(crate) type AuthSegmentID = u32;
pub(crate) type AuthChallenge = u64;

#[derive(Debug)]
pub struct AuthSegment {
    array: ArrayInSHM<AuthSegmentID, AuthChallenge, usize>,
}

impl AuthSegment {
    pub fn create(challenge: AuthChallenge, shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        let array = ArrayInSHM::<AuthSegmentID, AuthChallenge, usize>::create(
            1 + shm_protocols.len(),
            AUTH_SEGMENT_PREFIX,
        )?;
        unsafe {
            (*array.elem_mut(0)) = challenge;
            for elem in 1..array.elem_count() {
                (*array.elem_mut(elem)) = shm_protocols[elem - 1] as u64;
            }
        };
        Ok(Self { array })
    }

    pub fn open(id: AuthSegmentID) -> ZResult<Self> {
        let array = ArrayInSHM::open(id, AUTH_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }

    pub fn challenge(&self) -> AuthChallenge {
        unsafe { *self.array.elem(0) }
    }

    pub fn protocols(&self) -> Vec<ProtocolID> {
        let mut result = vec![];
        for elem in 1..self.array.elem_count() {
            result.push(unsafe { *self.array.elem(elem) as u32 });
        }
        result
    }

    pub fn id(&self) -> AuthSegmentID {
        self.array.id()
    }
}
