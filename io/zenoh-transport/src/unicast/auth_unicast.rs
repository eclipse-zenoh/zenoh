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
use zenoh_crypto::PseudoRng;
use zenoh_result::ZResult;

use super::auth_segment::{AuthChallenge, AuthSegment, AuthSegmentID};

/*************************************/
/*          Authenticator            */
/*************************************/
pub(crate) struct AuthUnicast {
    segment: AuthSegment,
}

impl AuthUnicast {
    pub fn new() -> ZResult<Self> {
        // Create a challenge for session establishment
        let mut prng = PseudoRng::from_entropy();
        let nonce = prng.gen();

        // allocate SHM segment with challenge
        let segment = AuthSegment::create(nonce)?;

        Ok(Self { segment })
    }

    pub fn challenge(&self) -> AuthChallenge {
        self.segment.challenge()
    }

    pub fn id(&self) -> AuthSegmentID {
        self.segment.id()
    }
}
