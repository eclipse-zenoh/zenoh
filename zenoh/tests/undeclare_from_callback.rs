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

//! A synchronous undeclare with `wait_callbacks()`, called from inside the
//! entity's own callback, never returns.
//!
//! # The mechanism
//!
//! `SyncGroup` is a `tokio::sync::Semaphore` initialised to `MAX_PERMITS`:
//!
//! * `SyncGroup::notifier()` takes **one** permit (`try_acquire_owned`).
//! * `SyncGroup::wait()` demands **all** of them back
//!   (`block_in_place(acquire_many(max_permits()))`).
//!
//! `Session::register_callback_drop_notifier` releases those permits from the
//! `Callback`'s *on-drop* closure. `Callback<T>` is `Clone` and its dropper is a
//! shared `Arc`, so the closure runs only when the **last clone** is dropped.
//!
//! Delivery calls the user through a live `Callback` clone. That clone keeps a
//! permit alive for the whole call. If the callback then asks for
//! `wait_callbacks()`, `acquire_many(MAX)` waits for a permit that only this
//! stack can release — and this stack is the one waiting. A deterministic
//! self-join: one thread, one call stack, no race, no lock.
//!
//! Reported downstream three times without ever being filed against zenoh:
//! Dexory's `~SubscriptionData` -> `ze_undeclare_advanced_subscriber`
//! (2 in 200 robot bring-ups), `ros2/rmw_zenoh#993` (`~ClientData` ->
//! `z_undeclare_querier`), and `ZettaScaleLabs/hiroz#284`, which names the
//! shape and states that its own tripwire cannot see it.
//!
//! The trigger is not exotic. `wait_callbacks` is opt-in in Rust, and zenoh-c
//! opts in at every one of its twelve teardown sites, session close included —
//! so every C and C++ binding above it, `rmw_zenoh_cpp` among them, takes this
//! path on every entity destruction.
//!
//! # Reading these tests
//!
//! A timeout alone proves nothing: "the callback never ran" and "the callback
//! ran and wedged" look identical from outside. Every scenario therefore
//! reports whether it entered the callback, and [`Outcome`] keeps *deadlocked*,
//! *panicked* and *completed* apart rather than collapsing them into a bool.
//!
//! Three controls keep the two headline tests honest:
//!
//! * [`undeclaring_without_wait_callbacks_from_the_callback_completes`] — the
//!   defect is the *wait*, not undeclaring from a callback.
//! * [`undeclaring_with_wait_callbacks_from_another_thread_completes`] — the
//!   wait works normally, and the harness can report `Completed` at all.
//! * [`the_callback_guard_fires_when_the_callback_never_runs`] — the
//!   entered-the-callback assertion is live rather than decorative.
//!
//! # The one that separates the candidate fixes
//!
//! [`undeclaring_an_unrelated_entity_from_a_callback_still_waits_for_it`] is not
//! a control. It pins a guarantee that holds today and that only *some* fixes
//! keep: tearing down entity B from inside entity A's callback, while B's own
//! callback runs elsewhere. Nothing is self-held, so the barrier is achievable
//! and `main` honours it. A fix keyed on "am I inside any callback?" cannot see
//! the difference and drops it; one keyed on "do I hold this permit?" keeps it.

// `wait_callbacks` is `#[zenoh_macros::internal_or_unstable]`. Without this gate
// the file does not compile in the default configuration, which CI builds.
#![cfg(any(feature = "unstable", feature = "internal"))]

use std::{
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use zenoh::{query::Query, sample::Sample, Wait};

/// Generous by design. The self-join is deterministic and immediate, so any
/// wait that survives scheduler noise settles it; the work itself is
/// microseconds.
const WAIT: Duration = Duration::from_secs(10);

/// How long to give local delivery before concluding the callback never ran.
const DELIVERY_GRACE: Duration = Duration::from_millis(500);

/// How long the unrelated entity's callback stays in flight.
///
/// Long enough that "the wait honoured the barrier" and "the wait returned
/// immediately" are hundreds of milliseconds apart, so the verdict does not
/// rest on scheduler noise.
const UNRELATED_CALLBACK_WORK: Duration = Duration::from_millis(800);

/// An isolated session: no multicast, no gossip, no listeners.
///
/// These scenarios are single-process and need no network. Left on defaults
/// they peer with anything else scouting on the machine — including the sibling
/// tests in this file when cargo runs them in parallel, which is enough to
/// break the control's "nothing is delivered here" premise.
fn isolated_config() -> zenoh::Config {
    let mut config = zenoh::Config::default();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .unwrap();
    config.insert_json5("listen/endpoints", "[]").unwrap();
    config
}

/// What happened to the scenario thread.
#[derive(Debug)]
enum Outcome {
    /// Ran to completion — no deadlock.
    Completed,
    /// Still blocked after [`WAIT`]. This is the self-join.
    Deadlocked,
    /// Unwound. Carries the message so a test can assert *which* failure.
    Panicked(String),
}

/// Whether the scenario reached its callback, and what became of its thread.
struct Report {
    outcome: Outcome,
    entered: Arc<AtomicBool>,
}

impl Report {
    /// The guard every scenario needs: without it a timeout is
    /// indistinguishable from "delivery never happened".
    fn assert_entered_callback(&self) {
        std::thread::sleep(DELIVERY_GRACE);
        assert!(
            self.entered.load(Ordering::SeqCst),
            "the callback never ran, so this scenario proves nothing about \
             undeclaring from inside one"
        );
    }
}

/// Runs `body` on its own thread and classifies the result.
///
/// Keeping `Panicked` apart from `Deadlocked` matters: a bare `recv_timeout`
/// error would report an assertion failure inside the scenario as a deadlock,
/// which is exactly the lie these tests exist to avoid.
fn run_scenario(body: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Report {
    let entered = Arc::new(AtomicBool::new(false));
    let entered_body = entered.clone();
    let (tx, rx) = mpsc::channel();
    // Leaked deliberately on the deadlock path: the thread is wedged inside
    // `SyncGroup::wait` and will never return, which is the thing being shown.
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| body(entered_body)));
        let _ = tx.send(result.map_err(|payload| {
            payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_owned())
        }));
    });
    let outcome = match rx.recv_timeout(WAIT) {
        Ok(Ok(())) => Outcome::Completed,
        Ok(Err(msg)) => Outcome::Panicked(msg),
        Err(mpsc::RecvTimeoutError::Timeout) => Outcome::Deadlocked,
        // `tx` is owned by the closure and dropped only when it finishes, and
        // the closure always sends first. Reaching this means the thread was
        // aborted outright.
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Outcome::Panicked("<thread aborted without sending>".to_owned())
        }
    };
    Report { outcome, entered }
}

/// Slot holding the entity so its own callback can tear it down.
///
/// `undeclare()` takes `self`, so the callback has to *own* the entity to reach
/// the path under test. The handler parameter is `()`: `.callback(..)` consumes
/// the stream itself and leaves no receiver behind.
type Slot<T> = Arc<Mutex<Option<T>>>;

/// Takes the entity out of its slot without holding the test's own mutex across
/// the undeclare.
///
/// This scoping is load-bearing for the diagnosis. Holding a `std::sync::Mutex`
/// across the call would make "your test deadlocked on its own lock" a live
/// reading of the result. It is not: no user lock is held when zenoh blocks.
fn take<T>(slot: &Slot<T>) -> Option<T> {
    slot.lock().unwrap().take()
}

// ---------------------------------------------------------------------------
// The defect
// ---------------------------------------------------------------------------

/// A subscriber undeclared with `wait_callbacks()` from inside its own sample
/// callback.
///
/// This is Dexory's production shape reduced to one process: the delivering
/// thread owns the last reference, so teardown runs on it, and the teardown
/// blocks on a permit that same thread holds.
#[test]
fn undeclaring_a_subscriber_with_wait_callbacks_from_its_own_callback_deadlocks() {
    let report = run_scenario(|entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();
        let slot: Slot<zenoh::pubsub::Subscriber<()>> = Arc::new(Mutex::new(None));

        let slot_cb = slot.clone();
        let sub = session
            .declare_subscriber("test/undeclare_from_callback/subscriber")
            .callback(move |_s: Sample| {
                entered.store(true, Ordering::SeqCst);
                if let Some(sub) = take(&slot_cb) {
                    // The self-join. This stack holds a live `Callback` clone,
                    // so its permit cannot be released until this call returns
                    // — and this call is waiting for that permit.
                    let _ = sub.undeclare().wait_callbacks().wait();
                }
            })
            .wait()
            .unwrap();
        *slot.lock().unwrap() = Some(sub);

        // Same-session delivery is inline on the publishing thread, so the
        // thread that parks is this one.
        session
            .put("test/undeclare_from_callback/subscriber", "trigger")
            .wait()
            .unwrap();
    });

    report.assert_entered_callback();
    assert!(
        matches!(report.outcome, Outcome::Deadlocked),
        "expected the undeclare to park this thread forever, got {:?}",
        report.outcome
    );
}

/// The same self-join through a different entity, to show it is a property of
/// `SyncGroup` rather than of subscribers.
///
/// `ros2/rmw_zenoh#993` is this shape on a querier; a queryable is the cheapest
/// second entity to reach from one process.
#[test]
fn undeclaring_a_queryable_with_wait_callbacks_from_its_own_callback_deadlocks() {
    let report = run_scenario(|entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();
        let slot: Slot<zenoh::query::Queryable<()>> = Arc::new(Mutex::new(None));

        let slot_cb = slot.clone();
        let queryable = session
            .declare_queryable("test/undeclare_from_callback/queryable")
            .callback(move |q: Query| {
                entered.store(true, Ordering::SeqCst);
                let _ = q.reply("test/undeclare_from_callback/queryable", "answer").wait();
                if let Some(qa) = take(&slot_cb) {
                    let _ = qa.undeclare().wait_callbacks().wait();
                }
            })
            .wait()
            .unwrap();
        *slot.lock().unwrap() = Some(queryable);

        // A local queryable is dispatched inline on the querying thread, so
        // this thread is the one that parks.
        let _ = session
            .get("test/undeclare_from_callback/queryable")
            .callback(|_r| {})
            .wait();
    });

    report.assert_entered_callback();
    assert!(
        matches!(report.outcome, Outcome::Deadlocked),
        "expected the undeclare to park this thread forever, got {:?}",
        report.outcome
    );
}

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------

/// The same undeclare from the same place, minus `wait_callbacks()`.
///
/// This is the discriminator. Without it a reader can conclude that undeclaring
/// from a callback is inherently unsafe; with it, the defect is pinned to the
/// wait. Reintroduce `.wait_callbacks()` here and this test hangs.
#[test]
fn undeclaring_without_wait_callbacks_from_the_callback_completes() {
    let report = run_scenario(|entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();
        let slot: Slot<zenoh::pubsub::Subscriber<()>> = Arc::new(Mutex::new(None));

        let slot_cb = slot.clone();
        let sub = session
            .declare_subscriber("test/undeclare_from_callback/no_wait")
            .callback(move |_s: Sample| {
                entered.store(true, Ordering::SeqCst);
                if let Some(sub) = take(&slot_cb) {
                    let _ = sub.undeclare().wait();
                }
            })
            .wait()
            .unwrap();
        *slot.lock().unwrap() = Some(sub);

        session
            .put("test/undeclare_from_callback/no_wait", "trigger")
            .wait()
            .unwrap();
    });

    report.assert_entered_callback();
    assert!(
        matches!(report.outcome, Outcome::Completed),
        "undeclare without wait_callbacks should return from inside a callback, got {:?}",
        report.outcome
    );
}

/// `wait_callbacks()` from a thread that is *not* the callback's.
///
/// The permit is held by a stack this thread does not own, so it is released
/// when that callback returns and the wait completes. This is the behaviour the
/// API promises, and it is why the fix cannot simply delete the wait.
#[test]
fn undeclaring_with_wait_callbacks_from_another_thread_completes() {
    let report = run_scenario(|entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();

        let sub = session
            .declare_subscriber("test/undeclare_from_callback/other_thread")
            .callback(move |_s: Sample| {
                entered.store(true, Ordering::SeqCst);
                // Long enough that the undeclare below genuinely has to wait,
                // rather than arriving after the callback happened to finish.
                std::thread::sleep(Duration::from_millis(300));
            })
            .wait()
            .unwrap();

        session
            .put("test/undeclare_from_callback/other_thread", "trigger")
            .wait()
            .unwrap();

        sub.undeclare().wait_callbacks().wait().unwrap();
    });

    report.assert_entered_callback();
    assert!(
        matches!(report.outcome, Outcome::Completed),
        "wait_callbacks from an unrelated thread must complete, got {:?}",
        report.outcome
    );
}

// ---------------------------------------------------------------------------
// The discriminator between the two candidate fixes
// ---------------------------------------------------------------------------

/// Undeclaring an **unrelated** entity from inside a callback, while that
/// entity's own callback is in flight on another thread.
///
/// This is the case the rest of the file does not reach, and the only one that
/// separates the two designs on the table.
///
/// Nothing is self-held here. The waiting stack is inside subscriber **A**'s
/// callback and holds a permit in *A*'s `SyncGroup`; the entity being torn down
/// is **B**, whose only permit is held by a callback running on a different
/// thread. That permit will be released when B's callback returns, so the
/// barrier `wait_callbacks()` promises is **achievable** — and today it is
/// honoured.
///
/// A thread-local callback-depth counter cannot tell this apart from the
/// self-join. It sees "this thread is inside some callback" and drains
/// asynchronously, so the wait returns while B's callback is still running and
/// the promise is quietly dropped. A check on the *identity* of the permit
/// holder keeps it.
///
/// So this test pins behaviour that is correct on `main` and that the coarse
/// fix regresses. It is not evidence against that fix on its own — the
/// guarantee may be judged not worth its cost — but the trade should be made
/// deliberately rather than discovered later.
#[test]
fn undeclaring_an_unrelated_entity_from_a_callback_still_waits_for_it() {
    // Written by B's callback when it finishes.
    let b_finished = Arc::new(AtomicBool::new(false));
    // Read the instant `wait()` returned: did it wait for B or not?
    let b_finished_when_wait_returned = Arc::new(AtomicBool::new(false));
    // Secondary evidence, and what the failure message quotes.
    let wait_took_ms = Arc::new(AtomicU64::new(u64::MAX));

    let b_finished_body = b_finished.clone();
    let observed_body = b_finished_when_wait_returned.clone();
    let took_body = wait_took_ms.clone();

    let report = run_scenario(move |entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();
        let b_running = Arc::new(AtomicBool::new(false));

        // Entity B: unrelated to A, with a deliberately slow callback.
        let b_running_cb = b_running.clone();
        let b_finished_cb = b_finished_body.clone();
        let sub_b = session
            .declare_subscriber("test/undeclare_from_callback/unrelated/b")
            .callback(move |_s: Sample| {
                b_running_cb.store(true, Ordering::SeqCst);
                std::thread::sleep(UNRELATED_CALLBACK_WORK);
                b_finished_cb.store(true, Ordering::SeqCst);
            })
            .wait()
            .unwrap();
        let slot_b: Slot<zenoh::pubsub::Subscriber<()>> = Arc::new(Mutex::new(Some(sub_b)));

        // Entity A: its callback tears B down and records what it saw.
        let slot_b_cb = slot_b.clone();
        let b_finished_probe = b_finished_body.clone();
        let observed_cb = observed_body.clone();
        let took_cb = took_body.clone();
        let _sub_a = session
            .declare_subscriber("test/undeclare_from_callback/unrelated/a")
            .callback(move |_s: Sample| {
                entered.store(true, Ordering::SeqCst);
                if let Some(b) = take(&slot_b_cb) {
                    let started = Instant::now();
                    let _ = b.undeclare().wait_callbacks().wait();
                    took_cb.store(started.elapsed().as_millis() as u64, Ordering::SeqCst);
                    // The whole experiment is this one read.
                    observed_cb.store(b_finished_probe.load(Ordering::SeqCst), Ordering::SeqCst);
                }
            })
            .wait()
            .unwrap();

        // Put B's callback in flight on a thread that is not this one.
        let publisher = session.clone();
        std::thread::spawn(move || {
            publisher
                .put("test/undeclare_from_callback/unrelated/b", "slow")
                .wait()
                .unwrap();
        });
        while !b_running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }

        // Now run A's callback on this thread, with B's still running.
        session
            .put("test/undeclare_from_callback/unrelated/a", "trigger")
            .wait()
            .unwrap();
    });

    report.assert_entered_callback();
    assert!(
        matches!(report.outcome, Outcome::Completed),
        "undeclaring an unrelated entity must not deadlock, got {:?}",
        report.outcome
    );

    let took = wait_took_ms.load(Ordering::SeqCst);
    assert!(
        b_finished_when_wait_returned.load(Ordering::SeqCst),
        "wait_callbacks() on an unrelated entity returned after {took} ms while that \
         entity's callback was still running. No permit of B's was held by the waiting \
         stack, so the barrier was achievable and has been dropped — the signature of a \
         fix that asks \"am I inside any callback?\" instead of \"do I hold this permit?\""
    );
    assert!(
        b_finished.load(Ordering::SeqCst),
        "B's callback never finished, so this scenario proves nothing"
    );
}

/// The control for the control: publish where the subscriber does not match and
/// prove `assert_entered_callback` actually fails.
///
/// Without this, a scenario whose callback silently stopped running would pass
/// the deadlock assertions for the wrong reason.
#[test]
fn the_callback_guard_fires_when_the_callback_never_runs() {
    let report = run_scenario(|entered| {
        let session = zenoh::open(isolated_config()).wait().unwrap();

        let _sub = session
            .declare_subscriber("test/undeclare_from_callback/guard/subscribed")
            .callback(move |_s: Sample| {
                entered.store(true, Ordering::SeqCst);
            })
            .wait()
            .unwrap();

        session
            .put("test/undeclare_from_callback/guard/elsewhere", "trigger")
            .wait()
            .unwrap();
    });

    assert!(
        matches!(report.outcome, Outcome::Completed),
        "the guard control should not itself deadlock, got {:?}",
        report.outcome
    );
    let fired = std::panic::catch_unwind(AssertUnwindSafe(|| report.assert_entered_callback()));
    assert!(
        fired.is_err(),
        "assert_entered_callback passed with no delivery, so it is not guarding anything"
    );
}
