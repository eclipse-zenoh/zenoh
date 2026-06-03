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
use zenoh_protocol::transport::TransportMessage;

use crate::samples::sample_transport_message_seed_messages;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<TransportMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

/// Fuzzes the outer transport parser and roundtrips any successfully decoded value.
pub fn exercise_transport_message(data: &[u8]) {
    if let Some(message) = decode_transport_message(data) {
        assert_transport_message_roundtrip(&message);
    }
}

/// Summarizes how one raw input behaves when decoded as a transport message.
pub fn analyze_transport_message(data: &[u8]) -> TransportMessageAnalysis {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<TransportMessage, _> = codec.read(&mut reader);
    let consumed = data.len() - reader.remaining();
    let trailing = data[consumed..].to_vec();

    match decoded {
        Ok(message) => TransportMessageAnalysis {
            input_len: data.len(),
            roundtrip_ok: transport_message_roundtrip_ok(&message),
            decoded: Some(message),
            consumed,
            trailing,
        },
        Err(_) => TransportMessageAnalysis {
            input_len: data.len(),
            decoded: None,
            consumed,
            trailing,
            roundtrip_ok: false,
        },
    }
}

/// Encodes the deterministic transport samples into the checked-in seed corpus.
pub(crate) fn transport_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    sample_transport_message_seed_messages()
        .into_iter()
        .map(|(name, msg)| (name, encode_transport_message(&msg)))
        .collect()
}

/// Tries to decode arbitrary bytes as a transport message.
fn decode_transport_message(data: &[u8]) -> Option<TransportMessage> {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<TransportMessage, _> = codec.read(&mut reader);
    decoded.ok()
}

/// Turns a decode success into a hard fuzzing assertion about codec stability.
fn assert_transport_message_roundtrip(message: &TransportMessage) {
    assert!(
        transport_message_roundtrip_ok(message),
        "re-encoding a decoded transport message should succeed and remain stable"
    );
}

/// Checks whether encode/decode preserves a decoded transport message exactly.
fn transport_message_roundtrip_ok(message: &TransportMessage) -> bool {
    let codec = Zenoh080::new();
    let mut encoded = Vec::new();
    let mut writer = encoded.writer();
    let write_result: Result<(), _> = codec.write(&mut writer, message);
    if write_result.is_err() {
        return false;
    }

    let mut rereader = encoded.reader();
    let Ok(redecoded): Result<TransportMessage, _> = codec.read(&mut rereader) else {
        return false;
    };

    *message == redecoded && !rereader.can_read()
}

/// Serializes one deterministic transport sample into raw corpus bytes.
fn encode_transport_message(message: &TransportMessage) -> Vec<u8> {
    let codec = Zenoh080::new();
    let mut bytes = Vec::new();
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, message)
        .expect("seed transport message encoding should succeed");
    bytes
}
