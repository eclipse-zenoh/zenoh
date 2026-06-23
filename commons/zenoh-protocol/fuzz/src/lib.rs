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
//! Shared helpers for the `zenoh-protocol` fuzz targets.
//!
//! This crate currently focuses on string-based protocol parsers and keeps the
//! `libFuzzer` entrypoints thin while providing deterministic seed generation
//! and corpus verification helpers for CI.

use std::{
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use zenoh_protocol::core::EndPoint;

const ENDPOINT_CORPUS_DIR: &str = "corpus/endpoint_from_str";

/// Human-readable analysis output for one `EndPoint::from_str` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndPointAnalysis {
    pub input_len: usize,
    pub input: String,
    pub decoded: Option<EndPoint>,
    pub roundtrip_ok: bool,
}

/// Runs the `EndPoint::from_str` fuzz harness on one arbitrary byte slice.
pub fn exercise_endpoint_from_str(data: &[u8]) {
    if let Some(endpoint) = decode_endpoint_from_str(data) {
        assert_endpoint_roundtrip(&endpoint);
    }
}

/// Produces a structured analysis of one arbitrary `EndPoint::from_str` input.
pub fn analyze_endpoint_from_str(data: &[u8]) -> EndPointAnalysis {
    let input = String::from_utf8_lossy(data).into_owned();
    let decoded = EndPoint::from_str(&input).ok();
    let roundtrip_ok = decoded.as_ref().is_some_and(endpoint_roundtrip_ok);

    EndPointAnalysis {
        input_len: data.len(),
        input,
        decoded,
        roundtrip_ok,
    }
}

/// Generates the seed corpus for the `endpoint_from_str` fuzz target.
pub fn write_endpoint_seed_corpus() -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(ENDPOINT_CORPUS_DIR);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in endpoint_seed_corpus() {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Verifies that the generated `endpoint_from_str` seed corpus matches the fixtures.
pub fn verify_endpoint_seed_corpus() -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(ENDPOINT_CORPUS_DIR);
    let expected = endpoint_seed_corpus();

    for (name, expected_bytes) in expected {
        let path = corpus_dir.join(name);
        let actual = fs::read(&path)?;
        assert_eq!(
            actual,
            expected_bytes,
            "seed corpus file {} is out of date",
            path.display()
        );
        exercise_endpoint_from_str(&actual);
    }

    Ok(())
}

fn decode_endpoint_from_str(data: &[u8]) -> Option<EndPoint> {
    let input = String::from_utf8_lossy(data);
    EndPoint::from_str(&input).ok()
}

fn assert_endpoint_roundtrip(endpoint: &EndPoint) {
    assert!(
        endpoint_roundtrip_ok(endpoint),
        "parsed EndPoint should survive parse/stringify/parse roundtrip: {endpoint}"
    );
}

fn endpoint_roundtrip_ok(endpoint: &EndPoint) -> bool {
    let Ok(reparsed) = EndPoint::from_str(endpoint.as_str()) else {
        return false;
    };

    &reparsed == endpoint
}

fn endpoint_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    endpoint_seed_strings()
        .into_iter()
        .map(|(name, endpoint)| (name, endpoint.as_bytes().to_vec()))
        .collect()
}

fn endpoint_seed_strings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("simple", "tcp/127.0.0.1:7447"),
        ("metadata", "tcp/127.0.0.1:7447?b=2;a=1"),
        ("config", "tcp/127.0.0.1:7447#B=2;A=1"),
        ("metadata_and_config", "tcp/127.0.0.1:7447?b=2;a=1#B=2;A=1"),
        ("multicast", "udp/224.0.0.224:7447"),
    ]
}
