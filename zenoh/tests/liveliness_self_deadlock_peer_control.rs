//! Candidate A, "fix shape" control: same topology as
//! `liveliness_self_deadlock_peer.rs` (peer mode, 300 tokens), but the
//! query uses an oversized `FifoChannel` (capacity > token count) instead
//! of the default handler. Must complete quickly with no hang -- proves the
//! root cause is specifically synchronous-replay-size vs. channel-capacity,
//! and that a larger handler capacity at the call site (the same mitigation
//! shape hiroz applied) resolves it at the zenoh-API level too.
//!
//! Own single-test file (own cargo test binary / OS process) so leaked
//! state from this test never contaminates another candidate_a test.

#[path = "liveliness_self_deadlock_support/mod.rs"]
mod support;

use support::{run_candidate_a, N_TOKENS};
use zenoh::config::WhatAmI;

#[test]
fn candidate_a_peer_mode_oversized_handler_control() {
    let outcome = run_candidate_a(WhatAmI::Peer, Some(N_TOKENS + 100));
    eprintln!("=== candidate_a_peer_mode_oversized_handler_control result: {outcome} ===");
    assert_eq!(
        outcome, "no-hang",
        "peer mode self-declare/self-query hung even with oversized handler"
    );
}
