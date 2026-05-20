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
use zenoh_codec_fuzz::analyze_scouting_message;

fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("usage: analyze_scouting_message \"[1, 2, 3]\"");
    let bytes = parse_bytes(&arg);
    let analysis = analyze_scouting_message(&bytes);

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

fn parse_bytes(input: &str) -> Vec<u8> {
    input
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|part| {
            let token = part.trim();
            if token.is_empty() {
                return None;
            }

            let value = if let Some(hex) = token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
            {
                u8::from_str_radix(hex, 16).expect("invalid hex byte")
            } else {
                token.parse::<u8>().expect("invalid byte")
            };
            Some(value)
        })
        .collect()
}
