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
//! Shared helpers for the `zenoh-codec` fuzz targets.
//!
//! This module keeps the actual `libFuzzer` entrypoint thin and provides:
//! - the reusable `TransportMessage`, structured `NetworkMessage`, and
//!   `ScoutingMessage` fuzz harnesses,
//! - deterministic seed corpus generation,
//! - corpus verification for CI and local checks.
//!
//! The seed corpora stay aligned with each target's input format:
//! - parser-oriented targets use real encoder output,
//! - structured targets use stable handcrafted `arbitrary` inputs.
//!
//! The harness deliberately checks two different properties:
//! - decode robustness: arbitrary input may decode or fail, but it must not panic,
//! - codec consistency: if bytes decode into a message value, that value should be
//!   serializable and stable across decode/encode/decode.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    ZSlice,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    common::ZExtBody,
    core::{Locator, Reliability, Resolution, WhatAmI, ZenohIdProto},
    network::{
        declare::{common::DeclareFinal, subscriber::DeclareSubscriber},
        interest::{InterestMode, InterestOptions},
        request::ext::QueryTarget,
        Declare, Interest, NetworkBody, NetworkMessage, Oam as NetworkOam, Push as NetworkPush,
        Request, Response, ResponseFinal,
    },
    scouting::{HelloProto, Scout, ScoutingBody, ScoutingMessage},
    transport::{
        batch_size, close, fragment, frame, init, join, oam, Close, Fragment, Frame, InitAck,
        InitSyn, Join, KeepAlive, Oam, OpenAck, OpenSyn, PrioritySn, TransportBody,
        TransportMessage,
    },
    zenoh::{PushBody, Put, Query, Reply, RequestBody, ResponseBody},
};

const TRANSPORT_CORPUS_DIR: &str = "corpus/transport_message";
const NETWORK_CORPUS_DIR: &str = "corpus/network_message";
const SCOUTING_CORPUS_DIR: &str = "corpus/scouting_message";

/// Human-readable analysis output for one `TransportMessage` input.
///
/// This is intended for local debugging of fuzz inputs and crash reproducers. It
/// reports whether decode succeeded, how many bytes were consumed, the decoded
/// message (when available), and whether the stronger roundtrip check succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<TransportMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

/// Human-readable analysis output for one `NetworkMessage` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<NetworkMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

/// Structured `arbitrary` input for the inner `NetworkMessage` fuzz target.
///
/// Unlike the transport and scouting targets, this one does not primarily fuzz a
/// raw parse boundary. It uses a smaller valid-state model so libFuzzer can spend
/// more energy exploring semantic `NetworkMessage` combinations instead of mostly
/// invalid wire prefixes that are already covered through `transport_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkMessageModel {
    DeclareSubscriber {
        id: u32,
        suffix: String,
    },
    DeclareFinal,
    Interest {
        id: u32,
        mode: InterestModeModel,
        options: InterestOptionsModel,
        restricted: bool,
    },
    Oam {
        id: u16,
        body: Vec<u8>,
    },
    PushDel,
    PushPut {
        payload: Vec<u8>,
    },
    Request {
        id: u32,
        name: String,
    },
    Response {
        rid: u32,
        payload: Vec<u8>,
    },
    ResponseFinal {
        rid: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterestModeModel {
    Final,
    Current,
    Future,
    CurrentFuture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterestOptionsModel {
    KeyExprs,
    All,
    KeyExprsAndSubscribers,
}

/// Human-readable analysis output for one `ScoutingMessage` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<ScoutingMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

impl<'a> Arbitrary<'a> for InterestModeModel {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        Ok(match u.arbitrary::<u8>()? % 4 {
            0 => Self::Final,
            1 => Self::Current,
            2 => Self::Future,
            _ => Self::CurrentFuture,
        })
    }
}

impl<'a> Arbitrary<'a> for InterestOptionsModel {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        Ok(match u.arbitrary::<u8>()? % 3 {
            0 => Self::KeyExprs,
            1 => Self::All,
            _ => Self::KeyExprsAndSubscribers,
        })
    }
}

impl<'a> Arbitrary<'a> for NetworkMessageModel {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        Ok(match u.arbitrary::<u8>()? % 9 {
            0 => Self::DeclareSubscriber {
                id: arbitrary_u32(u)?,
                suffix: arbitrary_small_identifier(u, 12, "subscriber")?,
            },
            1 => Self::DeclareFinal,
            2 => Self::Interest {
                id: arbitrary_u32(u)?,
                mode: u.arbitrary()?,
                options: u.arbitrary()?,
                restricted: u.arbitrary::<u8>()? & 1 == 1,
            },
            3 => Self::Oam {
                id: arbitrary_u16(u)?,
                body: arbitrary_small_bytes(u, 12)?,
            },
            4 => Self::PushDel,
            5 => Self::PushPut {
                payload: arbitrary_small_bytes(u, 16)?,
            },
            6 => Self::Request {
                id: arbitrary_u32(u)?,
                name: arbitrary_small_identifier(u, 12, "request")?,
            },
            7 => Self::Response {
                rid: arbitrary_u32(u)?,
                payload: arbitrary_small_bytes(u, 16)?,
            },
            _ => Self::ResponseFinal {
                rid: arbitrary_u32(u)?,
            },
        })
    }
}

/// Runs the `TransportMessage` fuzz harness on one arbitrary byte slice.
///
/// This combines two checks:
/// - a "network realism" check: arbitrary bytes may decode or fail to decode,
/// - a "codec consistency" check: a successfully decoded message must roundtrip
///   through encode/decode without changing meaning.
pub fn exercise_transport_message(data: &[u8]) {
    if let Some(message) = decode_transport_message(data) {
        assert_transport_message_roundtrip(&message);
    }
}

/// Produces a structured analysis of one arbitrary `TransportMessage` input.
///
/// Unlike the fuzz harness, this helper never panics on decode failure. It is
/// meant for local inspection of interesting inputs discovered during fuzzing.
pub fn analyze_transport_message(data: &[u8]) -> TransportMessageAnalysis {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<TransportMessage, _> = codec.read(&mut reader);
    let consumed = data.len() - reader.remaining();
    let trailing = data[consumed..].to_vec();

    match decoded {
        Ok(message) => {
            let roundtrip_ok = transport_message_roundtrip_ok(&message);
            TransportMessageAnalysis {
                input_len: data.len(),
                decoded: Some(message),
                consumed,
                trailing,
                roundtrip_ok,
            }
        }
        Err(_) => TransportMessageAnalysis {
            input_len: data.len(),
            decoded: None,
            consumed,
            trailing,
            roundtrip_ok: false,
        },
    }
}

/// Runs the `NetworkMessage` fuzz harness on one arbitrary byte slice.
///
/// This target interprets the bytes as a structured `arbitrary` model rather than
/// as raw wire-format input. That keeps the fuzzing effort focused on valid inner
/// `NetworkMessage` states, while raw transport parsing remains covered by the
/// outer `transport_message` target.
pub fn exercise_network_message(data: &[u8]) {
    if let Some((model, _, _)) = decode_network_message_model(data) {
        exercise_network_message_model(&model);
    }
}

/// Runs the structured `NetworkMessage` harness on one already-decoded model.
pub fn exercise_network_message_model(model: &NetworkMessageModel) {
    let message = build_network_message(model);
    assert_network_message_roundtrip(&message);
}

/// Produces a structured analysis of one arbitrary `NetworkMessage` model input.
pub fn analyze_network_message(data: &[u8]) -> NetworkMessageAnalysis {
    match decode_network_message_model(data) {
        Some((model, consumed, trailing)) => {
            let message = build_network_message(&model);
            let roundtrip_ok = network_message_roundtrip_ok(&message);
            NetworkMessageAnalysis {
                input_len: data.len(),
                decoded: Some(message),
                consumed,
                trailing,
                roundtrip_ok,
            }
        }
        None => NetworkMessageAnalysis {
            input_len: data.len(),
            decoded: None,
            consumed: 0,
            trailing: data.to_vec(),
            roundtrip_ok: false,
        },
    }
}

/// Runs the `ScoutingMessage` fuzz harness on one arbitrary byte slice.
pub fn exercise_scouting_message(data: &[u8]) {
    if let Some(message) = decode_scouting_message(data) {
        assert_scouting_message_roundtrip(&message);
    }
}

/// Produces a structured analysis of one arbitrary `ScoutingMessage` input.
pub fn analyze_scouting_message(data: &[u8]) -> ScoutingMessageAnalysis {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<ScoutingMessage, _> = codec.read(&mut reader);
    let consumed = data.len() - reader.remaining();
    let trailing = data[consumed..].to_vec();

    match decoded {
        Ok(message) => {
            let roundtrip_ok = scouting_message_roundtrip_ok(&message);
            ScoutingMessageAnalysis {
                input_len: data.len(),
                decoded: Some(message),
                consumed,
                trailing,
                roundtrip_ok,
            }
        }
        Err(_) => ScoutingMessageAnalysis {
            input_len: data.len(),
            decoded: None,
            consumed,
            trailing,
            roundtrip_ok: false,
        },
    }
}

/// Attempts to decode one raw byte slice as a `TransportMessage`.
///
/// Returning `None` is a normal outcome for arbitrary fuzz input and corresponds to
/// the decode-only, network-facing part of the harness.
fn decode_transport_message(data: &[u8]) -> Option<TransportMessage> {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<TransportMessage, _> = codec.read(&mut reader);
    decoded.ok()
}

/// Attempts to decode one raw byte slice into the structured `NetworkMessage`
/// fuzz model used by the `network_message` target.
fn decode_network_message_model(data: &[u8]) -> Option<(NetworkMessageModel, usize, Vec<u8>)> {
    let mut unstructured = Unstructured::new(data);
    let model = NetworkMessageModel::arbitrary(&mut unstructured).ok()?;
    let consumed = data.len() - unstructured.len();
    let trailing = data[consumed..].to_vec();
    Some((model, consumed, trailing))
}

/// Attempts to decode one raw byte slice as a `ScoutingMessage`.
fn decode_scouting_message(data: &[u8]) -> Option<ScoutingMessage> {
    let codec = Zenoh080::new();
    let mut reader = data.reader();
    let decoded: Result<ScoutingMessage, _> = codec.read(&mut reader);
    decoded.ok()
}

/// Checks the stronger codec-internal invariant used by the fuzz harness.
///
/// This is not meant to model literal network behavior. Real Zenoh peers do not
/// usually parse an incoming packet and then immediately serialize the exact same
/// packet back out unchanged. Instead, this asserts that once the decoder accepts a
/// `TransportMessage`, the codec can also serialize that value and decode it back
/// into the same semantic message.
fn assert_transport_message_roundtrip(message: &TransportMessage) {
    assert!(
        transport_message_roundtrip_ok(message),
        "re-encoding a decoded transport message should succeed and remain stable"
    );
}

/// Checks the stronger codec-internal invariant for decoded `NetworkMessage`s.
fn assert_network_message_roundtrip(message: &NetworkMessage) {
    assert!(
        network_message_roundtrip_ok(message),
        "re-encoding a decoded network message should succeed and remain stable"
    );
}

/// Checks the stronger codec-internal invariant for decoded `ScoutingMessage`s.
fn assert_scouting_message_roundtrip(message: &ScoutingMessage) {
    assert!(
        scouting_message_roundtrip_ok(message),
        "re-encoding a decoded scouting message should succeed and remain stable"
    );
}

/// Returns whether a decoded `TransportMessage` successfully survives
/// encode/decode roundtripping without changing meaning.
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

/// Returns whether a decoded `NetworkMessage` successfully survives
/// encode/decode roundtripping without changing meaning.
fn network_message_roundtrip_ok(message: &NetworkMessage) -> bool {
    let codec = Zenoh080::new();
    let mut encoded = Vec::new();
    let mut writer = encoded.writer();
    let write_result: Result<(), _> = codec.write(&mut writer, message);
    if write_result.is_err() {
        return false;
    }

    let mut rereader = encoded.reader();
    let Ok(redecoded): Result<NetworkMessage, _> = codec.read(&mut rereader) else {
        return false;
    };

    *message == redecoded && !rereader.can_read()
}

/// Returns whether a decoded `ScoutingMessage` successfully survives
/// encode/decode roundtripping without changing meaning.
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

/// Generates the seed corpus for the `transport_message` fuzz target.
///
/// Each file is created from a deterministic `TransportMessage` sample encoded by
/// the real codec. The returned paths are the files that were written on disk.
pub fn write_transport_seed_corpus() -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(TRANSPORT_CORPUS_DIR);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in transport_message_seed_corpus() {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Generates the seed corpus for the structured `network_message` fuzz target.
pub fn write_network_seed_corpus() -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(NETWORK_CORPUS_DIR);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in network_message_seed_corpus() {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Generates the seed corpus for the `scouting_message` fuzz target.
pub fn write_scouting_seed_corpus() -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(SCOUTING_CORPUS_DIR);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in scouting_message_seed_corpus() {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Generates every deterministic seed corpus for the `zenoh-codec` fuzz crate.
pub fn write_all_seed_corpora() -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    written.extend(write_transport_seed_corpus()?);
    written.extend(write_network_seed_corpus()?);
    written.extend(write_scouting_seed_corpus()?);
    Ok(written)
}

/// Verifies that the generated seed corpus on disk matches the current encoder.
///
/// This is mainly used in CI after `write_transport_seed_corpus()` has been run. It also
/// executes each corpus file through the fuzz harness to ensure the seeds remain
/// valid parser inputs.
pub fn verify_transport_seed_corpus() -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(TRANSPORT_CORPUS_DIR);
    let expected = transport_message_seed_corpus();

    for (name, expected_bytes) in expected {
        let path = corpus_dir.join(name);
        let actual = fs::read(&path)?;
        assert_eq!(
            actual,
            expected_bytes,
            "seed corpus file {} is out of date",
            path.display()
        );
        exercise_transport_message(&actual);
    }

    Ok(())
}

/// Verifies that the generated `network_message` seed corpus matches the model
/// encoding used by the structured fuzz target.
pub fn verify_network_seed_corpus() -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(NETWORK_CORPUS_DIR);
    let expected = network_message_seed_corpus();

    for (name, expected_bytes) in expected {
        let path = corpus_dir.join(name);
        let actual = fs::read(&path)?;
        assert_eq!(
            actual,
            expected_bytes,
            "seed corpus file {} is out of date",
            path.display()
        );
        exercise_network_message(&actual);
    }

    Ok(())
}

/// Verifies that the generated `scouting_message` seed corpus matches the encoder.
pub fn verify_scouting_seed_corpus() -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(SCOUTING_CORPUS_DIR);
    let expected = scouting_message_seed_corpus();

    for (name, expected_bytes) in expected {
        let path = corpus_dir.join(name);
        let actual = fs::read(&path)?;
        assert_eq!(
            actual,
            expected_bytes,
            "seed corpus file {} is out of date",
            path.display()
        );
        exercise_scouting_message(&actual);
    }

    Ok(())
}

/// Verifies every generated seed corpus for the `zenoh-codec` fuzz crate.
pub fn verify_all_seed_corpora() -> io::Result<()> {
    verify_transport_seed_corpus()?;
    verify_network_seed_corpus()?;
    verify_scouting_seed_corpus()?;
    Ok(())
}

/// Builds the encoded byte corpus for the checked-in `TransportMessage` seed set.
fn transport_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    transport_message_seed_messages()
        .into_iter()
        .map(|(name, msg)| (name, encode_transport_message(&msg)))
        .collect()
}

/// Builds the structured-input byte corpus for the `NetworkMessage` seed set.
fn network_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    network_message_seed_models()
        .into_iter()
        .map(|(name, model)| (name, encode_network_message_model(&model)))
        .collect()
}

/// Builds the encoded byte corpus for the `ScoutingMessage` seed set.
fn scouting_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    scouting_message_seed_messages()
        .into_iter()
        .map(|(name, message)| (name, encode_scouting_message(&message)))
        .collect()
}

/// Encodes one deterministic `TransportMessage` sample into raw fuzz input bytes.
fn encode_transport_message(message: &TransportMessage) -> Vec<u8> {
    let codec = Zenoh080::new();
    let mut bytes = Vec::new();
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, message)
        .expect("seed transport message encoding should succeed");
    bytes
}

/// Encodes one deterministic `ScoutingMessage` sample into raw fuzz input bytes.
fn encode_scouting_message(message: &ScoutingMessage) -> Vec<u8> {
    let codec = Zenoh080::new();
    let mut bytes = Vec::new();
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, message)
        .expect("seed scouting message encoding should succeed");
    bytes
}

/// Returns one deterministic sample for each `TransportBody` variant we currently fuzz.
fn transport_message_seed_messages() -> Vec<(&'static str, TransportMessage)> {
    let network_message = sample_push_network_message();

    vec![
        (
            "close",
            Close {
                reason: close::reason::INVALID,
                session: true,
            }
            .into(),
        ),
        ("fragment", sample_fragment().into()),
        ("frame", sample_frame(network_message).into()),
        ("init_ack", sample_init_ack().into()),
        ("init_syn", sample_init_syn().into()),
        ("join", sample_join().into()),
        ("keep_alive", KeepAlive.into()),
        ("oam", TransportBody::OAM(sample_oam()).into()),
        ("open_ack", sample_open_ack().into()),
        ("open_syn", sample_open_syn().into()),
    ]
}

/// Returns one deterministic structured input per `NetworkMessage` variant.
fn network_message_seed_models() -> Vec<(&'static str, NetworkMessageModel)> {
    vec![
        (
            "declare",
            NetworkMessageModel::DeclareSubscriber {
                id: 0x4142_4344,
                suffix: "subscriber".into(),
            },
        ),
        ("declare_final", NetworkMessageModel::DeclareFinal),
        (
            "interest",
            NetworkMessageModel::Interest {
                id: 0x3132_3334,
                mode: InterestModeModel::Current,
                options: InterestOptionsModel::All,
                restricted: true,
            },
        ),
        (
            "oam",
            NetworkMessageModel::Oam {
                id: 9,
                body: vec![0x99, 0x01],
            },
        ),
        ("push_del", NetworkMessageModel::PushDel),
        (
            "push",
            NetworkMessageModel::PushPut {
                payload: vec![0x11, 0x22, 0x33],
            },
        ),
        (
            "request",
            NetworkMessageModel::Request {
                id: 0x0102_0304,
                name: "request".into(),
            },
        ),
        (
            "response",
            NetworkMessageModel::Response {
                rid: 0x1112_1314,
                payload: vec![0x44, 0x55],
            },
        ),
        (
            "response_final",
            NetworkMessageModel::ResponseFinal { rid: 0x2122_2324 },
        ),
    ]
}

/// Converts one structured fuzz model into a valid `NetworkMessage`.
fn build_network_message(model: &NetworkMessageModel) -> NetworkMessage {
    match model {
        NetworkMessageModel::DeclareSubscriber { id, suffix } => {
            let mut message = sample_declare_network_message();
            let declare = match &mut message.body {
                NetworkBody::Declare(declare) => declare,
                _ => unreachable!("sample declare seed must decode as a declare message"),
            };
            declare.body =
                zenoh_protocol::network::DeclareBody::DeclareSubscriber(DeclareSubscriber {
                    id: *id,
                    wire_expr: format!("demo/{suffix}").into(),
                });
            message
        }
        NetworkMessageModel::DeclareFinal => sample_declare_final_network_message(),
        NetworkMessageModel::Interest {
            id,
            mode,
            options,
            restricted,
        } => {
            let mut message = sample_interest_network_message();
            let interest = match &mut message.body {
                NetworkBody::Interest(interest) => interest,
                _ => unreachable!("sample interest seed must decode as an interest message"),
            };
            interest.id = *id;
            interest.mode = match mode {
                InterestModeModel::Final => InterestMode::Final,
                InterestModeModel::Current => InterestMode::Current,
                InterestModeModel::Future => InterestMode::Future,
                InterestModeModel::CurrentFuture => InterestMode::CurrentFuture,
            };
            interest.options = match options {
                InterestOptionsModel::KeyExprs => InterestOptions::KEYEXPRS,
                InterestOptionsModel::All => InterestOptions::ALL,
                InterestOptionsModel::KeyExprsAndSubscribers => {
                    InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS
                }
            };
            interest.wire_expr = if *restricted {
                Some("demo/restricted".into())
            } else {
                None
            };
            if interest.mode == InterestMode::Final {
                interest.options = InterestOptions::empty();
                interest.wire_expr = None;
            }
            message
        }
        NetworkMessageModel::Oam { id, body } => {
            let mut message = sample_oam_network_message();
            let oam = match &mut message.body {
                NetworkBody::OAM(oam) => oam,
                _ => unreachable!("sample oam seed must decode as an oam message"),
            };
            oam.id = *id;
            oam.body = ZExtBody::ZBuf(body.clone().into());
            message
        }
        NetworkMessageModel::PushDel => sample_push_del_network_message(),
        NetworkMessageModel::PushPut { payload } => {
            let mut message = sample_push_network_message();
            let push = match &mut message.body {
                NetworkBody::Push(push) => push,
                _ => unreachable!("sample push seed must decode as a push message"),
            };
            push.payload = PushBody::from(Put {
                payload: payload.clone().into(),
                ..Put::default()
            });
            message
        }
        NetworkMessageModel::Request { id, name } => {
            let mut message = sample_request_network_message();
            let request = match &mut message.body {
                NetworkBody::Request(request) => request,
                _ => unreachable!("sample request seed must decode as a request message"),
            };
            request.id = *id;
            request.wire_expr = format!("demo/{name}").into();
            message
        }
        NetworkMessageModel::Response { rid, payload } => {
            let mut message = sample_response_network_message();
            let response = match &mut message.body {
                NetworkBody::Response(response) => response,
                _ => unreachable!("sample response seed must decode as a response message"),
            };
            response.rid = *rid;
            response.payload = ResponseBody::from(Reply {
                consolidation: zenoh_protocol::zenoh::ConsolidationMode::DEFAULT,
                ext_unknown: Vec::new(),
                payload: PushBody::from(Put {
                    payload: payload.clone().into(),
                    ..Put::default()
                }),
            });
            message
        }
        NetworkMessageModel::ResponseFinal { rid } => {
            let mut message = sample_response_final_network_message();
            let response_final = match &mut message.body {
                NetworkBody::ResponseFinal(response_final) => response_final,
                _ => {
                    unreachable!(
                        "sample response-final seed must decode as a response-final message"
                    )
                }
            };
            response_final.rid = *rid;
            message
        }
    }
}

/// Encodes one structured `NetworkMessage` seed into `arbitrary` input bytes.
fn encode_network_message_model(model: &NetworkMessageModel) -> Vec<u8> {
    let mut bytes = Vec::new();

    match model {
        NetworkMessageModel::DeclareSubscriber { id, suffix } => {
            bytes.push(0);
            bytes.extend(id.to_le_bytes());
            bytes.push(suffix.len() as u8);
            bytes.extend(suffix.as_bytes());
        }
        NetworkMessageModel::DeclareFinal => bytes.push(1),
        NetworkMessageModel::Interest {
            id,
            mode,
            options,
            restricted,
        } => {
            bytes.push(2);
            bytes.extend(id.to_le_bytes());
            bytes.push(match mode {
                InterestModeModel::Final => 0,
                InterestModeModel::Current => 1,
                InterestModeModel::Future => 2,
                InterestModeModel::CurrentFuture => 3,
            });
            bytes.push(match options {
                InterestOptionsModel::KeyExprs => 0,
                InterestOptionsModel::All => 1,
                InterestOptionsModel::KeyExprsAndSubscribers => 2,
            });
            bytes.push(u8::from(*restricted));
        }
        NetworkMessageModel::Oam { id, body } => {
            bytes.push(3);
            bytes.extend(id.to_le_bytes());
            bytes.push(body.len() as u8);
            bytes.extend(body);
        }
        NetworkMessageModel::PushDel => bytes.push(4),
        NetworkMessageModel::PushPut { payload } => {
            bytes.push(5);
            bytes.push(payload.len() as u8);
            bytes.extend(payload);
        }
        NetworkMessageModel::Request { id, name } => {
            bytes.push(6);
            bytes.extend(id.to_le_bytes());
            bytes.push(name.len() as u8);
            bytes.extend(name.as_bytes());
        }
        NetworkMessageModel::Response { rid, payload } => {
            bytes.push(7);
            bytes.extend(rid.to_le_bytes());
            bytes.push(payload.len() as u8);
            bytes.extend(payload);
        }
        NetworkMessageModel::ResponseFinal { rid } => {
            bytes.push(8);
            bytes.extend(rid.to_le_bytes());
        }
    }

    bytes
}

/// Returns one deterministic sample for each `ScoutingBody` variant we currently fuzz.
fn scouting_message_seed_messages() -> Vec<(&'static str, ScoutingMessage)> {
    vec![
        ("hello", sample_hello_scouting_message()),
        ("scout", sample_scout_scouting_message()),
    ]
}

/// Builds a small `NetworkMessage` payload used by the frame seed and network corpus.
fn sample_push_network_message() -> NetworkMessage {
    let put = Put {
        payload: vec![0x11, 0x22, 0x33].into(),
        ..Put::default()
    };
    let push = NetworkPush::from(PushBody::from(put));
    NetworkBody::Push(push).into()
}

/// Builds a small `NetworkMessage` payload that exercises the `PushBody::Del` path.
fn sample_push_del_network_message() -> NetworkMessage {
    let push = NetworkPush::from(PushBody::from(zenoh_protocol::zenoh::Del::default()));
    NetworkBody::Push(push).into()
}

/// Returns a deterministic `Request` network message with a simple query body.
fn sample_request_network_message() -> NetworkMessage {
    let request = Request {
        id: 0x0102_0304,
        wire_expr: "demo/request".into(),
        ext_qos: zenoh_protocol::network::request::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::request::ext::NodeIdType::DEFAULT,
        ext_target: QueryTarget::DEFAULT,
        ext_budget: None,
        ext_timeout: None,
        payload: RequestBody::from(Query::default()),
    };
    NetworkBody::Request(request).into()
}

/// Returns a deterministic `Response` network message with a simple reply body.
fn sample_response_network_message() -> NetworkMessage {
    let reply = Reply {
        consolidation: zenoh_protocol::zenoh::ConsolidationMode::DEFAULT,
        ext_unknown: Vec::new(),
        payload: PushBody::from(Put {
            payload: vec![0x44, 0x55].into(),
            ..Put::default()
        }),
    };
    let response = Response {
        rid: 0x1112_1314,
        wire_expr: "demo/response".into(),
        payload: ResponseBody::from(reply),
        ext_qos: zenoh_protocol::network::response::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_respid: None,
    };
    NetworkBody::Response(response).into()
}

/// Returns a deterministic `ResponseFinal` network message.
fn sample_response_final_network_message() -> NetworkMessage {
    let response_final = ResponseFinal {
        rid: 0x2122_2324,
        ext_qos: zenoh_protocol::network::response::ext::QoSType::DEFAULT,
        ext_tstamp: None,
    };
    NetworkBody::ResponseFinal(response_final).into()
}

/// Returns a deterministic `Interest` network message.
fn sample_interest_network_message() -> NetworkMessage {
    let interest = Interest {
        id: 0x3132_3334,
        mode: InterestMode::Current,
        options: InterestOptions::KEYEXPRS,
        wire_expr: None,
        ext_qos: zenoh_protocol::network::interest::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::interest::ext::NodeIdType::DEFAULT,
    };
    NetworkBody::Interest(interest).into()
}

/// Returns a deterministic `Declare` network message with a subscriber declaration body.
fn sample_declare_network_message() -> NetworkMessage {
    let declare = Declare {
        interest_id: None,
        ext_qos: zenoh_protocol::network::declare::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::declare::ext::NodeIdType::DEFAULT,
        body: zenoh_protocol::network::DeclareBody::DeclareSubscriber(DeclareSubscriber {
            id: 0x4142_4344,
            wire_expr: "demo/subscriber".into(),
        }),
    };
    NetworkBody::Declare(declare).into()
}

/// Returns a deterministic `Declare` network message with a final declaration body.
fn sample_declare_final_network_message() -> NetworkMessage {
    let declare = Declare {
        interest_id: None,
        ext_qos: zenoh_protocol::network::declare::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::declare::ext::NodeIdType::DEFAULT,
        body: zenoh_protocol::network::DeclareBody::DeclareFinal(DeclareFinal),
    };
    NetworkBody::Declare(declare).into()
}

/// Returns a deterministic network-level OAM message.
fn sample_oam_network_message() -> NetworkMessage {
    let oam = NetworkOam {
        id: 9,
        body: ZExtBody::ZBuf(vec![0x99, 0x01].into()),
        ext_qos: zenoh_protocol::network::oam::ext::QoSType::DEFAULT,
        ext_tstamp: None,
    };
    NetworkBody::OAM(oam).into()
}

/// Returns a deterministic `Scout` scouting message.
fn sample_scout_scouting_message() -> ScoutingMessage {
    ScoutingBody::Scout(Scout {
        version: 1,
        what: WhatAmI::Peer.into(),
        zid: Some(fixed_zid()),
    })
    .into()
}

/// Returns a deterministic `Hello` scouting message with one locator.
fn sample_hello_scouting_message() -> ScoutingMessage {
    let hello = HelloProto {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        locators: vec![Locator::new("tcp", "127.0.0.1:7447", "").expect("locator must be valid")],
    };
    ScoutingBody::Hello(hello).into()
}

/// Returns a deterministic `Frame` seed containing one valid network message.
fn sample_frame(network_message: NetworkMessage) -> Frame {
    Frame {
        reliability: Reliability::Reliable,
        sn: 0x0102_0304,
        ext_qos: frame::ext::QoSType::DEFAULT,
        payload: vec![network_message],
    }
}

/// Returns a deterministic single-fragment message seed.
fn sample_fragment() -> Fragment {
    Fragment {
        reliability: Reliability::Reliable,
        more: false,
        sn: 0x0506_0708,
        payload: ZSlice::from(vec![0xaa, 0xbb, 0xcc]),
        ext_qos: fragment::ext::QoSType::DEFAULT,
        ext_first: None,
        ext_drop: None,
    }
}

/// Returns a minimal deterministic `InitSyn` seed without optional extensions.
fn sample_init_syn() -> InitSyn {
    InitSyn {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::UNICAST,
        ext_qos: None,
        ext_qos_link: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_patch: init::ext::PatchType::NONE,
        ext_region_name: None,
    }
}

/// Returns a minimal deterministic `InitAck` seed with a fixed cookie.
fn sample_init_ack() -> InitAck {
    InitAck {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::UNICAST,
        cookie: vec![0xde, 0xad, 0xbe, 0xef].into(),
        ext_qos: None,
        ext_qos_link: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_patch: init::ext::PatchType::NONE,
        ext_region_name: None,
    }
}

/// Returns a deterministic `OpenSyn` seed with a fixed cookie and no extensions.
fn sample_open_syn() -> OpenSyn {
    OpenSyn {
        lease: Duration::from_millis(1_500),
        initial_sn: 0x1112_1314,
        cookie: vec![0xca, 0xfe, 0xba, 0xbe].into(),
        ext_qos: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_remote_bound: None,
    }
}

/// Returns a deterministic `OpenAck` seed with no optional extensions.
fn sample_open_ack() -> OpenAck {
    OpenAck {
        lease: Duration::from_secs(2),
        initial_sn: 0x2122_2324,
        ext_qos: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_remote_bound: None,
    }
}

/// Returns a deterministic multicast `Join` seed.
fn sample_join() -> Join {
    Join {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::MULTICAST,
        lease: Duration::from_secs(3),
        next_sn: PrioritySn::DEFAULT,
        ext_qos: None,
        ext_shm: None,
        ext_patch: join::ext::PatchType::NONE,
    }
}

/// Returns a deterministic transport-level OAM seed.
fn sample_oam() -> Oam {
    Oam {
        id: 7,
        body: ZExtBody::Z64(42),
        ext_qos: oam::ext::QoSType::DEFAULT,
    }
}

fn arbitrary_u32(u: &mut Unstructured<'_>) -> ArbitraryResult<u32> {
    let bytes = u.bytes(4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn arbitrary_u16(u: &mut Unstructured<'_>) -> ArbitraryResult<u16> {
    let bytes = u.bytes(2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn arbitrary_small_bytes(u: &mut Unstructured<'_>, max_len: usize) -> ArbitraryResult<Vec<u8>> {
    let len = (u.arbitrary::<u8>()? as usize) % (max_len + 1);
    Ok(u.bytes(len)?.to_vec())
}

fn arbitrary_small_identifier(
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

/// Returns a fixed non-random Zenoh ID so corpus generation stays deterministic.
fn fixed_zid() -> ZenohIdProto {
    ZenohIdProto::try_from([0x10, 0x20, 0x30, 0x40]).expect("fixed corpus ZenohId must be valid")
}
