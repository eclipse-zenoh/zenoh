//
// Copyright (c) 2026 ZettaScale Technology
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

use arbitrary::{Result as ArbitraryResult, Unstructured};
use zenoh_protocol::core::ZenohIdProto;

pub(crate) const TRANSPORT_CORPUS_DIR: &str = "corpus/transport_message";
pub(crate) const NETWORK_CORPUS_DIR: &str = "corpus/network_message";
pub(crate) const SCOUTING_CORPUS_DIR: &str = "corpus/scouting_message";

/// Decodes four arbitrary bytes into a little-endian `u32`.
pub(crate) fn arbitrary_u32(u: &mut Unstructured<'_>) -> ArbitraryResult<u32> {
    let bytes = u.bytes(4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Decodes two arbitrary bytes into a little-endian `u16`.
pub(crate) fn arbitrary_u16(u: &mut Unstructured<'_>) -> ArbitraryResult<u16> {
    let bytes = u.bytes(2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Produces a bounded byte vector so the structured models stay small and fast.
pub(crate) fn arbitrary_small_bytes(
    u: &mut Unstructured<'_>,
    max_len: usize,
) -> ArbitraryResult<Vec<u8>> {
    let len = (u.arbitrary::<u8>()? as usize) % (max_len + 1);
    Ok(u.bytes(len)?.to_vec())
}

/// Produces a bounded ASCII-ish identifier string for deterministic wire expr names.
pub(crate) fn arbitrary_small_identifier(
    u: &mut Unstructured<'_>,
    max_len: usize,
    fallback: &str,
) -> ArbitraryResult<String> {
    let len = (u.arbitrary::<u8>()? as usize) % (max_len + 1);
    let bytes = u.bytes(len)?;
    let mut out = String::with_capacity(bytes.len().max(fallback.len()));

    for &byte in bytes {
        let ch = match byte % 37 {
            0..=9 => char::from(b'0' + (byte % 10)),
            10..=35 => char::from(b'a' + (byte % 26)),
            _ => '_',
        };
        out.push(ch);
    }

    if out.is_empty() {
        out.push_str(fallback);
    }

    Ok(out)
}

/// Returns the fixed Zenoh ID used by deterministic seed samples.
pub(crate) fn fixed_zid() -> ZenohIdProto {
    ZenohIdProto::try_from([0x10, 0x20, 0x30, 0x40]).expect("fixed corpus ZenohId must be valid")
}
