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

use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::scouting::ScoutingMessage;

use crate::samples::{sample_hello_scouting_message, sample_scout_scouting_message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<ScoutingMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

/// Fuzzes the scouting parser and roundtrips any accepted scouting message.
pub fn exercise_scouting_message(data: &[u8]) {
    if let Some(message) = decode_scouting_message(data) {
        assert_scouting_message_roundtrip(&message);
    }
}

/// Summarizes how one raw input behaves when decoded as a scouting message.
pub fn analyze_scouting_message(data: &[u8]) -> ScoutingMessageAnalysis {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<ScoutingMessage, _> = codec.read(&mut reader);
    let consumed = data.len() - reader.remaining();
    let trailing = data[consumed..].to_vec();

    match decoded {
        Ok(message) => ScoutingMessageAnalysis {
            input_len: data.len(),
            roundtrip_ok: scouting_message_roundtrip_ok(&message),
            decoded: Some(message),
            consumed,
            trailing,
        },
        Err(_) => ScoutingMessageAnalysis {
            input_len: data.len(),
            decoded: None,
            consumed,
            trailing,
            roundtrip_ok: false,
        },
    }
}

/// Encodes the deterministic scouting samples into the checked-in seed corpus.
pub(crate) fn scouting_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    scouting_message_seed_messages()
        .into_iter()
        .map(|(name, message)| (name, encode_scouting_message(&message)))
        .collect()
}

/// Returns one deterministic sample per scouting message family we fuzz.
fn scouting_message_seed_messages() -> Vec<(&'static str, ScoutingMessage)> {
    vec![
        ("hello", sample_hello_scouting_message()),
        ("scout", sample_scout_scouting_message()),
    ]
}

/// Tries to decode arbitrary bytes as a scouting message.
fn decode_scouting_message(data: &[u8]) -> Option<ScoutingMessage> {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<ScoutingMessage, _> = codec.read(&mut reader);
    decoded.ok()
}

/// Turns a decode success into a hard fuzzing assertion about codec stability.
fn assert_scouting_message_roundtrip(message: &ScoutingMessage) {
    assert!(
        scouting_message_roundtrip_ok(message),
        "re-encoding a decoded scouting message should succeed and remain stable"
    );
}

/// Checks whether encode/decode preserves a decoded scouting message exactly.
fn scouting_message_roundtrip_ok(message: &ScoutingMessage) -> bool {
    let codec = Zenoh080::new();
    let mut encoded = Vec::new();
    let mut writer = encoded.writer();
    let write_result: Result<(), _> = codec.write(&mut writer, message);
    if write_result.is_err() {
        return false;
    }

    let mut rereader = encoded.reader();
    let Ok(redecoded): Result<ScoutingMessage, _> = codec.read(&mut rereader) else {
        return false;
    };

    *message == redecoded && !rereader.can_read()
}

/// Serializes one deterministic scouting sample into raw corpus bytes.
fn encode_scouting_message(message: &ScoutingMessage) -> Vec<u8> {
    let codec = Zenoh080::new();
    let mut bytes = Vec::new();
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, message)
        .expect("seed scouting message encoding should succeed");
    bytes
}
