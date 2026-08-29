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

use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    common::ZExtBody,
    network::{
        interest::{InterestMode, InterestOptions},
        DeclareBody, NetworkBody, NetworkMessage,
    },
    zenoh::{PushBody, Put, Reply, ResponseBody},
};

use crate::{
    samples::{
        sample_declare_final_network_message, sample_declare_network_message,
        sample_interest_network_message, sample_oam_network_message,
        sample_push_del_network_message, sample_push_network_message,
        sample_request_network_message, sample_response_final_network_message,
        sample_response_network_message,
    },
    util::{arbitrary_small_bytes, arbitrary_small_identifier, arbitrary_u16, arbitrary_u32},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMessageAnalysis {
    pub input_len: usize,
    pub decoded: Option<NetworkMessage>,
    pub consumed: usize,
    pub trailing: Vec<u8>,
    pub roundtrip_ok: bool,
}

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

/// Generates a small valid interest-mode state from arbitrary bytes.
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

/// Generates a small valid interest-options state from arbitrary bytes.
impl<'a> Arbitrary<'a> for InterestOptionsModel {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        Ok(match u.arbitrary::<u8>()? % 3 {
            0 => Self::KeyExprs,
            1 => Self::All,
            _ => Self::KeyExprsAndSubscribers,
        })
    }
}

/// Maps arbitrary bytes into one bounded valid network-message model variant.
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

/// Fuzzes the structured network-message model decoded from raw arbitrary bytes.
pub fn exercise_network_message(data: &[u8]) {
    if let Some((model, _, _)) = decode_network_message_model(data) {
        exercise_network_message_model(&model);
    }
}

/// Fuzzes one already-decoded structured model as a valid network message.
pub fn exercise_network_message_model(model: &NetworkMessageModel) {
    let message = build_network_message(model);
    assert_network_message_roundtrip(&message);
}

/// Summarizes how one arbitrary model input behaves as a network message.
pub fn analyze_network_message(data: &[u8]) -> NetworkMessageAnalysis {
    match decode_network_message_model(data) {
        Some((model, consumed, trailing)) => {
            let message = build_network_message(&model);
            NetworkMessageAnalysis {
                input_len: data.len(),
                roundtrip_ok: network_message_roundtrip_ok(&message),
                decoded: Some(message),
                consumed,
                trailing,
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

/// Encodes the deterministic structured models into the checked-in seed corpus.
pub(crate) fn network_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    network_message_seed_models()
        .into_iter()
        .map(|(name, model)| (name, encode_network_message_model(&model)))
        .collect()
}

/// Tries to decode raw arbitrary bytes into the structured network-message model.
fn decode_network_message_model(data: &[u8]) -> Option<(NetworkMessageModel, usize, Vec<u8>)> {
    let mut unstructured = Unstructured::new(data);
    let model = NetworkMessageModel::arbitrary(&mut unstructured).ok()?;
    let consumed = data.len() - unstructured.len();
    let trailing = data[consumed..].to_vec();
    Some((model, consumed, trailing))
}

/// Turns a decode success into a hard fuzzing assertion about codec stability.
fn assert_network_message_roundtrip(message: &NetworkMessage) {
    assert!(
        network_message_roundtrip_ok(message),
        "re-encoding a decoded network message should succeed and remain stable"
    );
}

/// Checks whether encode/decode preserves a decoded network message exactly.
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

/// Returns one deterministic structured model per network-message family we fuzz.
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

/// Converts the structured model into a concrete valid `NetworkMessage`.
fn build_network_message(model: &NetworkMessageModel) -> NetworkMessage {
    match model {
        NetworkMessageModel::DeclareSubscriber { id, suffix } => {
            let mut message = sample_declare_network_message();
            let declare = match &mut message.body {
                NetworkBody::Declare(declare) => declare,
                _ => unreachable!("sample declare seed must decode as a declare message"),
            };
            declare.body = DeclareBody::DeclareSubscriber(
                zenoh_protocol::network::declare::subscriber::DeclareSubscriber {
                    id: *id,
                    wire_expr: format!("demo/{suffix}").into(),
                },
            );
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
                _ => unreachable!(
                    "sample response-final seed must decode as a response-final message"
                ),
            };
            response_final.rid = *rid;
            message
        }
    }
}

/// Serializes one structured model into raw bytes for the seed corpus.
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
