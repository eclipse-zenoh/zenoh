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
use std::{env, process};

use zenoh_codec_fuzz::analyze_transport_message;

fn main() {
    let Some(raw) = env::args().nth(1) else {
        eprintln!(
            "usage: cargo run --manifest-path commons/zenoh-codec/fuzz/Cargo.toml --bin analyze_transport_message -- \"[2, 220, 11, 13, 0]\""
        );
        process::exit(2);
    };

    let bytes = parse_byte_list(&raw).unwrap_or_else(|err| {
        eprintln!("failed to parse input: {err}");
        process::exit(2);
    });

    let analysis = analyze_transport_message(&bytes);
    println!("input_len: {}", analysis.input_len);
    println!("decode_ok: {}", analysis.decoded.is_some());
    println!("consumed: {}", analysis.consumed);
    println!("trailing_len: {}", analysis.trailing.len());
    println!("trailing: {:?}", analysis.trailing);
    println!("roundtrip_ok: {}", analysis.roundtrip_ok);
    if let Some(message) = analysis.decoded {
        println!("decoded_message: {message:#?}");
    }
}

fn parse_byte_list(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed)
        .trim();

    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|part| {
            let token = part.trim();
            let value = if let Some(hex) = token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
            {
                u16::from_str_radix(hex, 16).map_err(|_| format!("invalid hex byte: {token}"))?
            } else {
                token
                    .parse::<u16>()
                    .map_err(|_| format!("invalid byte: {token}"))?
            };

            u8::try_from(value).map_err(|_| format!("byte out of range: {token}"))
        })
        .collect()
}
