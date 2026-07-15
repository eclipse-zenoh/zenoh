//! Minimal single-process repro for the liveliness_query synchronous-replay
//! self-deadlock (Candidate A).
//!
//! A single `Session` (peer or router mode, fully offline -- no listen /
//! connect endpoints) declares many liveliness tokens ON ITSELF, then calls
//! `session.liveliness().get(pattern).wait()` against its OWN local table.
//! Tokens declared by a session are visible in that same session's local
//! interest/routing table synchronously, with no settle-time or network
//! delay required, so `send_interest`'s synchronous replay walk in
//! `dispatcher::face::send_interest` fires the full token fan-out into the
//! query's reply channel on the calling thread before
//! `liveliness_query()`/`.wait()` can return. With the default bounded FIFO
//! handler (256 slots, `API_DATA_RECEPTION_CHANNEL_SIZE` in
//! `zenoh/src/api/session.rs`) and a token count above that capacity, the
//! synchronous `flume::Sender::send` blocks forever waiting for a receiver
//! that cannot run until the very call that would start it returns --
//! a deterministic self-deadlock.
//!
//! Each sub-test runs the risky call on a background thread and joins with a
//! bounded wall-clock timeout so a real hang fails the test cleanly instead
//! of wedging the test process. Companion tests using an oversized
//! `FifoChannel` capacity prove the same topology completes quickly once the
//! channel can never fill during the synchronous replay -- the same
//! mitigation shape as the hiroz-side fix.

use std::sync::mpsc;
use std::time::Duration;
use zenoh::config::{Config, WhatAmI};
use zenoh::handlers::FifoChannel;
use zenoh::Wait;

const N_TOKENS: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(15);

/// `handler_capacity = None` uses the default (256-slot) bounded FIFO
/// handler -- the vulnerable path. `Some(cap)` uses an explicit oversized
/// `FifoChannel`, the companion "fix shape" case that must never hang.
fn run_candidate_a(whatami: WhatAmI, handler_capacity: Option<usize>) -> &'static str {
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = rt.block_on(async {
            let mut config = Config::default();
            config.set_mode(Some(whatami)).unwrap();
            // No network: no listen/connect endpoints configured.
            let session = zenoh::open(config).await.unwrap();

            let mut tokens = Vec::with_capacity(N_TOKENS);
            for i in 0..N_TOKENS {
                let ke = format!("test/liveliness/selfdeadlock/{i}");
                let tok = session.liveliness().declare_token(ke).await.unwrap();
                tokens.push(tok);
            }
            eprintln!("[candidate-a] declared {} tokens", tokens.len());

            // This is the synchronous, blocking call under test. The
            // hypothesis is that it self-deadlocks regardless of executor
            // because it's the *same* calling thread doing both the
            // declare-replay writes and (would-be) the channel drain, and
            // replay happens before the function returns.
            eprintln!(
                "[candidate-a] issuing liveliness_query (blocking wait), handler={:?}...",
                handler_capacity
            );
            let query = session.liveliness().get("test/liveliness/selfdeadlock/**");
            let replies = match handler_capacity {
                Some(cap) => query.with(FifoChannel::new(cap)).wait(),
                None => query.wait(),
            };

            match replies {
                Ok(replies) => {
                    let mut count = 0;
                    // Drain with a short per-recv timeout too, in case get()
                    // itself returned but the channel is wedged some other
                    // way.
                    loop {
                        match replies.recv_timeout(Duration::from_secs(5)) {
                            Ok(_reply) => count += 1,
                            Err(_) => break,
                        }
                    }
                    eprintln!("[candidate-a] drained {count} replies");
                    count
                }
                Err(e) => {
                    eprintln!("[candidate-a] get() errored: {e:?}");
                    0
                }
            }
        });

        let _ = tx.send(result);
    });

    match rx.recv_timeout(TIMEOUT) {
        Ok(count) => {
            eprintln!("[candidate-a] completed normally, {count} replies, no hang");
            // best-effort join, don't block test exit
            let _ = handle.join();
            "no-hang"
        }
        Err(_) => {
            eprintln!(
                "[candidate-a] TIMED OUT after {:?} waiting for liveliness_query to return -- \
                 background thread still parked, NOT joining (would wedge the test process)",
                TIMEOUT
            );
            // Deliberately do not join() -- it would hang the test process too.
            // Leak the thread; process exit will reap it.
            std::mem::forget(handle);
            "HUNG"
        }
    }
}

/// Vulnerable path: default (256-slot) bounded FIFO handler, peer mode.
#[test]
fn candidate_a_peer_mode() {
    let outcome = run_candidate_a(WhatAmI::Peer, None);
    eprintln!("=== candidate_a_peer_mode result: {outcome} ===");
    assert_eq!(outcome, "no-hang", "peer mode self-declare/self-query hung");
}

/// Vulnerable path: default (256-slot) bounded FIFO handler, router mode.
#[test]
fn candidate_a_router_mode() {
    let outcome = run_candidate_a(WhatAmI::Router, None);
    eprintln!("=== candidate_a_router_mode result: {outcome} ===");
    assert_eq!(outcome, "no-hang", "router mode self-declare/self-query hung");
}

/// Companion "fix shape" case: same topology (peer mode, N_TOKENS = 300),
/// but with an oversized `FifoChannel` (capacity > N_TOKENS) so the
/// synchronous replay can never fill the channel. Must complete quickly and
/// must not hang -- proves the root cause is specifically channel capacity
/// vs. synchronous replay size, not something inherent to self-declare
/// self-query topology.
#[test]
fn candidate_a_peer_mode_oversized_handler_control() {
    let outcome = run_candidate_a(WhatAmI::Peer, Some(N_TOKENS + 100));
    eprintln!("=== candidate_a_peer_mode_oversized_handler_control result: {outcome} ===");
    assert_eq!(
        outcome, "no-hang",
        "peer mode self-declare/self-query hung even with oversized handler"
    );
}

/// Companion "fix shape" case, router mode.
#[test]
fn candidate_a_router_mode_oversized_handler_control() {
    let outcome = run_candidate_a(WhatAmI::Router, Some(N_TOKENS + 100));
    eprintln!("=== candidate_a_router_mode_oversized_handler_control result: {outcome} ===");
    assert_eq!(
        outcome, "no-hang",
        "router mode self-declare/self-query hung even with oversized handler"
    );
}
