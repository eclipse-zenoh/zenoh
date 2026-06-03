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
//! The crate keeps the actual `libFuzzer` entrypoints thin and exposes:
//! - target-specific harnesses for transport, structured network, and scouting,
//! - deterministic seed-corpus generation,
//! - corpus verification for CI and local checks.

mod corpus;
mod network_message;
mod samples;
mod scouting_message;
mod transport_message;
mod util;

pub use corpus::{
    verify_all_seed_corpora, verify_network_seed_corpus, verify_scouting_seed_corpus,
    verify_transport_seed_corpus, write_all_seed_corpora, write_network_seed_corpus,
    write_scouting_seed_corpus, write_transport_seed_corpus,
};
pub use network_message::{
    analyze_network_message, exercise_network_message, exercise_network_message_model,
    InterestModeModel, InterestOptionsModel, NetworkMessageAnalysis, NetworkMessageModel,
};
pub use scouting_message::{
    analyze_scouting_message, exercise_scouting_message, ScoutingMessageAnalysis,
};
pub use transport_message::{
    analyze_transport_message, exercise_transport_message, TransportMessageAnalysis,
};
