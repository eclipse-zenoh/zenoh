//! Candidate A, vulnerable path (regression test post-fix): single `Session`
//! in router mode declares 300 liveliness tokens on itself, then queries
//! its own local table with the default (256-slot) bounded FIFO handler.
//! Pre-fix, this self-deadlocked (see KNOWLEDGE.md); post-fix (async
//! replay in `Face::send_interest`), it must complete promptly and deliver
//! every reply.
//!
//! Deliberately its own single-test file (its own cargo test binary / OS
//! process, not merged with `liveliness_self_deadlock_peer.rs`) so this
//! test's deliberately-leaked session/tokens/thread can never contaminate
//! another test -- running both in the same process previously caused real
//! cross-test interference (leaked sessions autoconnecting to each other
//! via scouting/multicast, which is enabled by default).

use std::{sync::mpsc, time::Duration};

use zenoh::{
    config::{Config, WhatAmI},
    Wait,
};

const N_TOKENS: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn candidate_a_router_mode() {
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = rt.block_on(async {
            let mut config = Config::default();
            config.set_mode(Some(WhatAmI::Router)).unwrap();
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
                    while replies.recv_timeout(Duration::from_secs(5)).is_ok() {
                        count += 1;
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
            // teardown cost. This test is its own cargo test binary (own
            // OS process), so leaking here cannot contaminate any other
            // test.
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

    eprintln!("=== candidate_a_router_mode result: {outcome}, {reply_count} replies ===");
    assert_eq!(
        outcome, "no-hang",
        "router mode self-declare/self-query hung"
    );
    assert_eq!(
        reply_count, N_TOKENS,
        "expected all {N_TOKENS} tokens to be replayed as replies, got {reply_count}"
    );
}
