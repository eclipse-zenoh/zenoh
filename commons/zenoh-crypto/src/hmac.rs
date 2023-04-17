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
use hmac::{Hmac, Mac};
use sha3::{Digest, Sha3_256};
use zenoh_result::ZResult;

pub fn sign(key: &[u8], data: &[u8]) -> ZResult<Vec<u8>> {
    let mut hmac = Hmac::<Sha3_256>::new_from_slice(key)?;
    hmac.update(data);
    Ok(hmac.finalize().into_bytes().as_slice().to_vec())
}

pub fn digest(data: &[u8]) -> Vec<u8> {
    Sha3_256::digest(data).as_slice().to_vec()
}
