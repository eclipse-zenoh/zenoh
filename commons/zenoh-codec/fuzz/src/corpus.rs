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

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    network_message::{exercise_network_message, network_message_seed_corpus},
    scouting_message::{exercise_scouting_message, scouting_message_seed_corpus},
    transport_message::{exercise_transport_message, transport_message_seed_corpus},
    util::{NETWORK_CORPUS_DIR, SCOUTING_CORPUS_DIR, TRANSPORT_CORPUS_DIR},
};

/// Regenerates the deterministic transport-message corpus on disk.
pub fn write_transport_seed_corpus() -> io::Result<Vec<PathBuf>> {
    write_seed_corpus(TRANSPORT_CORPUS_DIR, transport_message_seed_corpus())
}

/// Regenerates the structured network-message corpus on disk.
pub fn write_network_seed_corpus() -> io::Result<Vec<PathBuf>> {
    write_seed_corpus(NETWORK_CORPUS_DIR, network_message_seed_corpus())
}

/// Regenerates the deterministic scouting-message corpus on disk.
pub fn write_scouting_seed_corpus() -> io::Result<Vec<PathBuf>> {
    write_seed_corpus(SCOUTING_CORPUS_DIR, scouting_message_seed_corpus())
}

/// Regenerates every seed corpus used by the codec fuzz crate.
pub fn write_all_seed_corpora() -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    written.extend(write_transport_seed_corpus()?);
    written.extend(write_network_seed_corpus()?);
    written.extend(write_scouting_seed_corpus()?);
    Ok(written)
}

/// Verifies the transport-message corpus matches current encoder output.
pub fn verify_transport_seed_corpus() -> io::Result<()> {
    verify_seed_corpus(
        TRANSPORT_CORPUS_DIR,
        transport_message_seed_corpus(),
        exercise_transport_message,
    )
}

/// Verifies the structured network-message corpus matches current model encoding.
pub fn verify_network_seed_corpus() -> io::Result<()> {
    verify_seed_corpus(
        NETWORK_CORPUS_DIR,
        network_message_seed_corpus(),
        exercise_network_message,
    )
}

/// Verifies the scouting-message corpus matches current encoder output.
pub fn verify_scouting_seed_corpus() -> io::Result<()> {
    verify_seed_corpus(
        SCOUTING_CORPUS_DIR,
        scouting_message_seed_corpus(),
        exercise_scouting_message,
    )
}

/// Verifies every checked-in seed corpus used by the codec fuzz crate.
pub fn verify_all_seed_corpora() -> io::Result<()> {
    verify_transport_seed_corpus()?;
    verify_network_seed_corpus()?;
    verify_scouting_seed_corpus()?;
    Ok(())
}

/// Writes one target's corpus entries into its on-disk corpus directory.
fn write_seed_corpus(
    corpus_dir: &str,
    entries: Vec<(&'static str, Vec<u8>)>,
) -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(corpus_dir);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in entries {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Confirms one target's on-disk corpus still matches its expected entries.
fn verify_seed_corpus(
    corpus_dir: &str,
    expected: Vec<(&'static str, Vec<u8>)>,
    exercise: fn(&[u8]),
) -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(corpus_dir);

    for (name, expected_bytes) in expected {
        let path = corpus_dir.join(name);
        let actual = fs::read(&path)?;
        assert_eq!(
            actual,
            expected_bytes,
            "seed corpus file {} is out of date",
            path.display()
        );
        exercise(&actual);
    }

    Ok(())
}
