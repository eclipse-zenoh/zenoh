//! Minimal single-process repro attempt for the liveliness_query synchronous-replay deadlock.
//!
//! Hypothesis (Candidate A): a single Session (peer or router mode) that declares >256
//! liveliness tokens ON ITSELF, then calls session.liveliness().get(pattern).wait() against
//! its OWN local table, might trigger the synchronous fifo-channel-full self-deadlock
//! without any network/gossip/multi-process machinery.
//!
//! Each sub-test runs the risky call on a background thread and joins with a bounded
//! timeout so a real hang doesn't wedge the test runner.

use std::sync::mpsc;
use std::time::Duration;
use zenoh::config::{Config, WhatAmI};
use zenoh::Wait;

const N_TOKENS: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(15);

fn run_candidate_a(whatami: WhatAmI) -> &'static str {
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

            // This is the synchronous, blocking call under test. It happens on the tokio
            // worker thread here since we called it inside block_on — but the hypothesis
            // is that it self-deadlocks regardless of executor because it's the *same*
            // calling thread doing both the declare-replay writes and (would-be) the
            // channel drain, and replay happens before the function returns.
            eprintln!("[candidate-a] issuing liveliness_query (blocking wait)...");
            let replies = session
                .liveliness()
                .get("test/liveliness/selfdeadlock/**")
                .wait();

            match replies {
                Ok(replies) => {
                    let mut count = 0;
                    // Drain with a short per-recv timeout too, in case get() itself
                    // returned but the channel is wedged some other way.
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

#[test]
fn candidate_a_peer_mode() {
    let outcome = run_candidate_a(WhatAmI::Peer);
    eprintln!("=== candidate_a_peer_mode result: {outcome} ===");
    assert_eq!(outcome, "no-hang", "peer mode self-declare/self-query hung");
}

#[test]
fn candidate_a_router_mode() {
    let outcome = run_candidate_a(WhatAmI::Router);
    eprintln!("=== candidate_a_router_mode result: {outcome} ===");
    assert_eq!(outcome, "no-hang", "router mode self-declare/self-query hung");
}
