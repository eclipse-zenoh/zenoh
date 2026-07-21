//! Regression test: a peer-mode `Session` declares 300 liveliness tokens on
//! itself, then queries its own local table with the default 256-slot
//! handler -- the exact path that self-deadlocked pre-fix (see
//! KNOWLEDGE.md). Must complete promptly and deliver every reply.
//!
//! Own test binary (own OS process) so its deliberately-leaked
//! session/tokens/thread can't autoconnect to and contaminate
//! `liveliness_self_deadlock_router.rs` via default scouting/multicast.

use std::{sync::mpsc, time::Duration};

use zenoh::{
    config::{Config, WhatAmI},
    Wait,
};

const N_TOKENS: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn candidate_a_peer_mode() {
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = rt.block_on(async {
            let mut config = Config::default();
            config.set_mode(Some(WhatAmI::Peer)).unwrap();
            // No network: no listen/connect endpoints configured.
            let session = zenoh::open(config).await.unwrap();

            let mut tokens = Vec::with_capacity(N_TOKENS);
            for i in 0..N_TOKENS {
                let ke = format!("test/liveliness/selfdeadlock/{i}");
                let tok = session.liveliness().declare_token(ke).await.unwrap();
                tokens.push(tok);
            }
            eprintln!("[candidate-a] declared {} tokens", tokens.len());

            // The blocking call under test: replay and the reply-channel
            // drain would run on this same thread, and replay happens
            // before the call returns.
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

            // Leak deliberately: dropping 300 tokens triggers its own
            // synchronous cleanup, which would run before `block_on`
            // returns and confound "did the query hang" with "did
            // teardown hang". Safe to leak -- own process, exits after.
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

    // Never join(): the Runtime's own drop would block on teardown,
    // which is unrelated to (and could wedge) the query outcome we
    // already have. Leak the thread; process exit reaps it.
    std::mem::forget(handle);

    eprintln!("=== candidate_a_peer_mode result: {outcome}, {reply_count} replies ===");
    assert_eq!(outcome, "no-hang", "peer mode self-declare/self-query hung");
    assert_eq!(
        reply_count, N_TOKENS,
        "expected all {N_TOKENS} tokens to be replayed as replies, got {reply_count}"
    );
}
