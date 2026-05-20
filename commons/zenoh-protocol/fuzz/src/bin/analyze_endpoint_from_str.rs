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
use zenoh_protocol_fuzz::analyze_endpoint_from_str;

fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("usage: analyze_endpoint_from_str <endpoint-string>");
    let analysis = analyze_endpoint_from_str(arg.as_bytes());

    println!("input_len: {}", analysis.input_len);
    println!("input: {:?}", analysis.input);
    println!("decode_ok: {}", analysis.decoded.is_some());
    println!("roundtrip_ok: {}", analysis.roundtrip_ok);
    if let Some(endpoint) = analysis.decoded {
        println!("decoded_endpoint: {endpoint:#?}");
    }
}
