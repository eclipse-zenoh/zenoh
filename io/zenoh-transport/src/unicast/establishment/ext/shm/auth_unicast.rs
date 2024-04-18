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
use std::ops::Deref;
use zenoh_crypto::PseudoRng;
use zenoh_result::ZResult;
use zenoh_shm::api::common::types::ProtocolID;

use super::auth_segment::AuthSegment;

/*************************************/
/*          Authenticator            */
/*************************************/
pub(crate) struct AuthUnicast {
    segment: AuthSegment,
}

impl Deref for AuthUnicast {
    type Target = AuthSegment;

    fn deref(&self) -> &Self::Target {
        &self.segment
    }
}

impl AuthUnicast {
    pub fn new(shm_protocols: &[ProtocolID]) -> ZResult<Self> {
        // Create a challenge for session establishment
        let mut prng = PseudoRng::from_entropy();
        let nonce = prng.gen();

        // allocate SHM segment with challenge
        let segment = AuthSegment::create(nonce, shm_protocols)?;

        Ok(Self { segment })
    }
}
