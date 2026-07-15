//! Candidate A, vulnerable path: single `Session` in router mode declares
//! 300 liveliness tokens on itself, then queries its own local table with
//! the default (256-slot) bounded FIFO handler. Expected: HUNG.
//!
//! See `liveliness_self_deadlock_support/mod.rs` for the full mechanism.
//! Split into its own single-test file (its own cargo test binary / OS
//! process) so this test's deliberately-leaked session/tokens/thread can
//! never contaminate any other candidate_a test.

#[path = "liveliness_self_deadlock_support/mod.rs"]
mod support;

use support::run_candidate_a;
use zenoh::config::WhatAmI;

#[test]
fn candidate_a_router_mode() {
    let outcome = run_candidate_a(WhatAmI::Router, None);
    eprintln!("=== candidate_a_router_mode result: {outcome} ===");
    assert_eq!(outcome, "no-hang", "router mode self-declare/self-query hung");
}
