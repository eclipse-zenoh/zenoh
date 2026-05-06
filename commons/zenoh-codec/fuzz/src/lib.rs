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
//! - the reusable `TransportMessage` fuzz harness,
//! - deterministic seed corpus generation,
//! - corpus verification for CI and local checks.
//!
//! The corpus is generated from real encoder output instead of being hand-authored.
//! That keeps the inputs aligned with the current wire format and makes it easy to
//! refresh them when the codec changes.
//!
//! The harness deliberately checks two different properties:
//! - decode robustness: arbitrary input may decode or fail, but it must not panic,
//! - codec consistency: if bytes decode into a `TransportMessage`, that value should
//!   be serializable and stable across decode/encode/decode.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    ZSlice,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    common::ZExtBody,
    core::{Reliability, Resolution, WhatAmI, ZenohIdProto},
    network::{NetworkBody, NetworkMessage, Push},
    transport::{
        batch_size, close, fragment, frame, init, join, oam, Close, Fragment, Frame, InitAck,
        InitSyn, Join, KeepAlive, Oam, OpenAck, OpenSyn, PrioritySn, TransportBody,
        TransportMessage,
    },
    zenoh::{PushBody, Put},
};

const CORPUS_DIR: &str = "corpus/transport_message";

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

/// Checks the stronger codec-internal invariant used by the fuzz harness.
///
/// This is not meant to model literal network behavior. Real Zenoh peers do not
/// usually parse an incoming packet and then immediately serialize the exact same
/// packet back out unchanged. Instead, this asserts that once the decoder accepts a
/// `TransportMessage`, the codec can also serialize that value and decode it back
/// into the same semantic message.
fn assert_transport_message_roundtrip(message: &TransportMessage) {
    let codec = Zenoh080::new();
    let mut encoded = Vec::new();
    let mut writer = encoded.writer();
    let write_result: Result<(), _> = codec.write(&mut writer, message);
    write_result.expect("re-encoding a decoded transport message should succeed");

    let mut rereader = encoded.reader();
    let redecoded: TransportMessage = codec
        .read(&mut rereader)
        .expect("re-decoding a re-encoded transport message should succeed");

    assert_eq!(*message, redecoded);
    assert!(
        !rereader.can_read(),
        "re-encoded transport message should consume its full buffer"
    );
}

/// Generates the seed corpus for the `transport_message` fuzz target.
///
/// Each file is created from a deterministic `TransportMessage` sample encoded by
/// the real codec. The returned paths are the files that were written on disk.
pub fn write_seed_corpus() -> io::Result<Vec<PathBuf>> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS_DIR);
    fs::create_dir_all(&corpus_dir)?;

    let mut written = Vec::new();
    for (name, bytes) in transport_message_seed_corpus() {
        let path = corpus_dir.join(name);
        fs::write(&path, bytes)?;
        written.push(path);
    }

    Ok(written)
}

/// Verifies that the generated seed corpus on disk matches the current encoder.
///
/// This is mainly used in CI after `write_seed_corpus()` has been run. It also
/// executes each corpus file through the fuzz harness to ensure the seeds remain
/// valid parser inputs.
pub fn verify_seed_corpus() -> io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS_DIR);
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

/// Builds the encoded byte corpus for the checked-in `TransportMessage` seed set.
fn transport_message_seed_corpus() -> Vec<(&'static str, Vec<u8>)> {
    transport_message_seed_messages()
        .into_iter()
        .map(|(name, msg)| (name, encode_transport_message(&msg)))
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

/// Returns one deterministic sample for each `TransportBody` variant we currently fuzz.
fn transport_message_seed_messages() -> Vec<(&'static str, TransportMessage)> {
    let network_message = sample_network_message();

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

/// Builds a small `NetworkMessage` payload used by the frame seed.
fn sample_network_message() -> NetworkMessage {
    let put = Put {
        payload: vec![0x11, 0x22, 0x33].into(),
        ..Put::default()
    };
    let push = Push::from(PushBody::from(put));
    NetworkBody::Push(push).into()
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

/// Returns a fixed non-random Zenoh ID so corpus generation stays deterministic.
fn fixed_zid() -> ZenohIdProto {
    ZenohIdProto::try_from([0x10, 0x20, 0x30, 0x40]).expect("fixed corpus ZenohId must be valid")
}
