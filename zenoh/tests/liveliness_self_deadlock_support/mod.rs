//! Shared helper for the Candidate A liveliness self-deadlock repro tests.
//! Not a test binary itself (lives in a subdirectory, so cargo's default
//! `tests/*.rs` auto-discovery skips it) -- included via `#[path = ...] mod
//! support;` from each of the four single-test files that ARE separate
//! cargo test binaries (their own OS processes), so that leaked
//! sessions/tokens/threads from one test can never contaminate another.

use std::sync::mpsc;
use std::time::Duration;
use zenoh::config::{Config, WhatAmI};
use zenoh::Wait;

pub const N_TOKENS: usize = 300;
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Declares `N_TOKENS` liveliness tokens on a session, then issues a
/// liveliness query against its own local table using the default
/// (256-slot) bounded FIFO handler -- the vulnerable path.
///
/// Returns `(outcome, reply_count)`: `outcome` is `"no-hang"` if the query
/// returned and was drained within `TIMEOUT`, `"HUNG"` otherwise.
/// `reply_count` is the number of replies actually drained (`0` if the
/// query hung, since the background thread's result is never observed in
/// that case).
pub fn run_candidate_a(whatami: WhatAmI) -> (&'static str, usize) {
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
            eprintln!("[candidate-a] issuing liveliness_query (blocking wait)...");
            let replies = session
                .liveliness()
                .get("test/liveliness/selfdeadlock/**")
                .wait();

            let count = match replies {
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
            };

            // Leak `tokens` and `session` deliberately. Dropping 300
            // LivelinessTokens (and the Session itself) triggers a second
            // round of synchronous undeclare/cleanup work that can itself
            // take a long time (or hang) -- and since Drop runs as part of
            // this async block's scope exit, it would run *before*
            // `rt.block_on` returns to the outer thread, confounding
            // "did the query hang" with "did cleanup hang". Leaking here
            // decouples the two: the moment the query genuinely completes
            // (or doesn't), that's what determines the test's timing, not
            // teardown cost. Each candidate_a_* test is its own cargo test
            // binary (own OS process), so leaking here cannot contaminate
            // any other test.
            std::mem::forget(tokens);
            std::mem::forget(session);

            count
        });

        let _ = tx.send(result);
    });

    let (outcome, reply_count) = match rx.recv_timeout(TIMEOUT) {
        Ok(count) => {
            eprintln!("[candidate-a] completed normally, {count} replies, no hang");
            ("no-hang", count)
        }
        Err(_) => {
            eprintln!(
                "[candidate-a] TIMED OUT after {:?} waiting for liveliness_query to return -- \
                 background thread still parked",
                TIMEOUT
            );
            ("HUNG", 0)
        }
    };

    // Deliberately never join() the background thread, in either branch.
    // The query outcome is already known from the channel recv above; the
    // background thread's tokio Runtime, when it eventually drops (in the
    // no-hang case) or never does (in the HUNG case), blocks until all of
    // the session's own spawned background tasks terminate -- which is
    // teardown cost unrelated to the query itself and must not be allowed
    // to confound (or wedge) this test's pass/fail signal. Leak the thread;
    // process exit reaps it.
    std::mem::forget(handle);
    (outcome, reply_count)
}
