//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use crate::core::{ZError, ZErrorKind, ZResult};
use crate::zerror2;
use hmac::{Hmac, Mac, NewMac};
use sha3::Sha3_256;

pub fn sign(key: &[u8], data: &[u8]) -> ZResult<Vec<u8>> {
    let mut hmac = Hmac::<Sha3_256>::new_varkey(&key).map_err(|e| {
        zerror2!(ZErrorKind::Other {
            descr: e.to_string()
        })
    })?;
    hmac.update(&data);
    Ok(hmac.finalize().into_bytes().as_slice().to_vec())
}
