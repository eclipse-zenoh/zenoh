//! Candidate A, vulnerable path (regression test post-fix): single `Session`
//! in router mode declares 300 liveliness tokens on itself, then queries
//! its own local table with the default (256-slot) bounded FIFO handler.
//! Pre-fix, this self-deadlocked (see KNOWLEDGE.md); post-fix (async
//! replay in `Face::send_interest`), it must complete promptly and deliver
//! every reply.
//!
//! See `liveliness_self_deadlock_support/mod.rs` for the full mechanism.
//! Split into its own single-test file (its own cargo test binary / OS
//! process) so this test's deliberately-leaked session/tokens/thread can
//! never contaminate any other candidate_a test.

#[path = "liveliness_self_deadlock_support/mod.rs"]
mod support;

use support::{run_candidate_a, N_TOKENS};
use zenoh::config::WhatAmI;

#[test]
fn candidate_a_router_mode() {
    let (outcome, reply_count) = run_candidate_a(WhatAmI::Router);
    eprintln!("=== candidate_a_router_mode result: {outcome}, {reply_count} replies ===");
    assert_eq!(outcome, "no-hang", "router mode self-declare/self-query hung");
    assert_eq!(
        reply_count, N_TOKENS,
        "expected all {N_TOKENS} tokens to be replayed as replies, got {reply_count}"
    );
}
