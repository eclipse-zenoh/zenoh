//
// Candidate B repro: two sessions in the same process, connected via an
// EXPLICIT direct TCP link (connect.endpoints), gossip and multicast
// scouting fully disabled on both sides.
//
// Session A (Peer, listener) declares N liveliness tokens on itself.
// Session B (Peer, connector, direct link only, no discovery) issues
// `liveliness().get(pattern).wait()` with the DEFAULT handler (256-slot
// bounded FIFO) against A's tokens, which must propagate to B purely via
// the direct TCP link's interest/declare mechanism (no gossip).
//
// Two sub-variants:
//   - `immediate`: B queries right after connecting, racing propagation.
//   - `settled`: B first declares its own liveliness subscriber and spins
//     until it has independently observed all N tokens locally, THEN
//     issues the query -- controlling for the timing variable.
//
// Everything runs on a background OS thread with a bounded wall-clock
// timeout so a genuine deadlock fails the test cleanly instead of wedging
// the whole `cargo test` process.

#![cfg(feature = "unstable")]

use std::{
    sync::mpsc,
    thread,
    time::Duration,
};

use zenoh::{config::WhatAmI, Config, Wait};

const N_TOKENS: usize = 300;
const LIVELINESS_PREFIX: &str = "test/liveliness/deadlock/repro";
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

fn make_a_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config
        .listen
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.scouting.gossip.set_enabled(Some(false)).unwrap();
    config
}

fn make_b_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    // Explicit direct link ONLY -- no listen endpoints, no discovery.
    config
        .connect
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}")
            .parse::<zenoh_protocol::core::EndPoint>()
            .unwrap()
            .into()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.scouting.gossip.set_enabled(Some(false)).unwrap();
    config
}

/// Runs the core repro body on the calling thread. `settle_before_query`
/// selects between the "immediate" and "settled" sub-variants.
fn run_repro(settle_before_query: bool, handler_capacity: Option<usize>) {
    let port = zenoh_test::get_free_tcp_port();

    let session_a = zenoh::open(make_a_config(port)).wait().unwrap();
    println!("[A] zid = {}", session_a.zid());

    // Declare N liveliness tokens on session A, on itself.
    let mut tokens = Vec::with_capacity(N_TOKENS);
    for i in 0..N_TOKENS {
        let ke = format!("{LIVELINESS_PREFIX}/{i}");
        let token = session_a.liveliness().declare_token(ke).wait().unwrap();
        tokens.push(token);
    }
    println!("[A] declared {N_TOKENS} liveliness tokens");

    let session_b = zenoh::open(make_b_config(port)).wait().unwrap();
    println!("[B] zid = {}", session_b.zid());

    if settle_before_query {
        // Independently confirm B has already received all N tokens
        // locally (e.g. via its own subscriber) BEFORE issuing the query,
        // to control for the timing/settle variable.
        let sub = session_b
            .liveliness()
            .declare_subscriber(format!("{LIVELINESS_PREFIX}/**"))
            .wait()
            .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = 0usize;
        while seen < N_TOKENS && std::time::Instant::now() < deadline {
            if let Ok(Some(_sample)) = sub.recv_timeout(Duration::from_millis(200)) {
                seen += 1;
            }
        }
        println!("[B] settled: observed {seen}/{N_TOKENS} tokens via subscriber before query");
        sub.undeclare().wait().unwrap();
        assert_eq!(seen, N_TOKENS, "did not observe all tokens before querying");
    }

    println!(
        "[B] issuing liveliness().get() with {} handler, settle_before_query={settle_before_query}",
        match handler_capacity {
            Some(c) => format!("bounded({c})"),
            None => "DEFAULT (256)".to_string(),
        }
    );

    let query = session_b.liveliness().get(format!("{LIVELINESS_PREFIX}/**"));
    let replies = if let Some(cap) = handler_capacity {
        query
            .with(zenoh::handlers::FifoChannel::new(cap))
            .wait()
            .unwrap()
    } else {
        // Default handler = bounded FIFO, capacity 256.
        query.wait().unwrap()
    };

    let mut count = 0;
    while let Ok(reply) = replies.recv() {
        if reply.result().is_ok() {
            count += 1;
        }
    }
    println!("[B] drained {count} replies");
    assert_eq!(count, N_TOKENS, "did not receive all tokens in reply");

    for token in tokens {
        token.undeclare().wait().unwrap();
    }
    session_b.close().wait().unwrap();
    session_a.close().wait().unwrap();
}

/// Spawns `run_repro` on a background thread and enforces a wall-clock
/// timeout, reporting hang vs completion vs panic.
fn run_bounded(name: &str, settle_before_query: bool, handler_capacity: Option<usize>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_repro(settle_before_query, handler_capacity)
        }));
        let _ = tx.send(result);
    });

    match rx.recv_timeout(TEST_TIMEOUT) {
        Ok(Ok(())) => {
            println!("[{name}] completed within timeout -- NO HANG");
        }
        Ok(Err(e)) => {
            // Don't join; thread panicked, that's fine, propagate as test failure.
            std::panic::resume_unwind(e);
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "[{name}] TIMED OUT after {:?} -- deadlock reproduced (thread still alive: {})",
                TEST_TIMEOUT,
                !handle.is_finished()
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("[{name}] worker thread died without sending a result");
        }
    }
}

/// Candidate B, immediate variant, default (256) bounded handler.
/// Expected (per prior hiroz findings on direct-link / non-gossip paths):
/// TBD -- this is the very case under test.
#[test]
fn candidate_b_immediate_default_handler() {
    zenoh_util::init_log_from_env_or("error");
    run_bounded("candidate_b_immediate_default_handler", false, None);
}

/// Candidate B, settled variant (subscriber-confirmed all tokens observed
/// locally before query), default (256) bounded handler.
#[test]
fn candidate_b_settled_default_handler() {
    zenoh_util::init_log_from_env_or("error");
    run_bounded("candidate_b_settled_default_handler", true, None);
}

/// Control: oversized handler capacity should never hang, immediate variant.
#[test]
fn candidate_b_immediate_oversized_handler_control() {
    zenoh_util::init_log_from_env_or("error");
    run_bounded(
        "candidate_b_immediate_oversized_handler_control",
        false,
        Some(N_TOKENS + 100),
    );
}
