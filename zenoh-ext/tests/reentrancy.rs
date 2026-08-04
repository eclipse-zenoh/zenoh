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

//! Re-entrancy of `AdvancedSubscriber`'s sample callback.
//!
//! `advanced_subscriber.rs` used to take `zlock!(statesref)` and call the user's
//! sample callback while that guard was live. Any callback re-entering an
//! `AdvancedSubscriber` API taking the same mutex then self-deadlocked on a
//! non-reentrant `std::sync::Mutex` — one thread, no race, every time.
//!
//! The reachable re-entrant surface is the sample-miss listener:
//!
//! * `SampleMissListenerBuilder::wait()` → `zlock!(statesref).register_miss_callback(..)`
//! * `SampleMissListener::drop`          → `zlock!(statesref).unregister_miss_callback(..)`
//!
//! The second is the nastier one: *dropping* a value inside a callback is not
//! something a user would think of as re-entering the middleware.
//!
//! Both now complete. Deliveries are staged into `State::outbox` under the guard
//! and drained once it is released, so these tests pin the fix rather than the
//! defect: reintroduce a `callback.call(..)` under `statesref` and they hang
//! again.
//!
//! # Reading these tests
//!
//! A timeout on its own proves nothing — "the callback never ran" and "the
//! callback ran and wedged" look identical from outside. Each scenario therefore
//! asserts it actually entered the callback, and [`Outcome`] keeps *deadlocked*,
//! *panicked* and *completed* apart instead of collapsing them into a bool.
//! That distinction matters more now, not less: a scenario whose callback
//! silently stopped running would otherwise *pass* against the fixed code.
//! `the_callback_guard_fires_when_the_callback_never_runs` is the control that
//! proves that guard is live rather than decorative.

#![cfg(feature = "unstable")]

use std::{
    any::Any,
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use zenoh::{sample::Sample, Wait};
use zenoh_ext::{AdvancedSubscriber, AdvancedSubscriberBuilderExt, Miss};

/// Generous by design. The deadlock is deterministic and immediate, so any wait
/// that survives scheduler noise proves the point; the work being done is
/// microseconds.
const WAIT: Duration = Duration::from_secs(10);

/// How long to give local delivery before concluding the callback never ran.
const DELIVERY_GRACE: Duration = Duration::from_millis(500);

/// An isolated session: no multicast, no gossip, no listeners.
///
/// These scenarios are single-process and need no network at all. Left on
/// defaults they peer with anything else scouting on the machine — including
/// the sibling tests in this file when cargo runs them in parallel, which is
/// enough to break the control's "nothing is delivered here" premise.
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

/// Slot holding a value the callback can drop without having created it.
/// `Box<dyn Any + Send>` avoids naming the listener's handler type.
type Slot = Arc<Mutex<Option<Box<dyn Any + Send>>>>;

/// The subscriber under test, stored so its own callback can re-enter it.
///
/// The handler parameter is `()`: `.callback(..)` consumes the sample stream
/// itself, leaving no receiver behind.
type SubSlot = Arc<Mutex<Option<AdvancedSubscriber<()>>>>;

/// What happened to the scenario thread.
#[derive(Debug)]
enum Outcome {
    /// Ran to completion — no deadlock.
    Completed,
    /// Still blocked after [`WAIT`]. This is the deadlock.
    Deadlocked,
    /// Unwound. Carries the panic message so a test can assert *which* failure.
    Panicked(String),
}

/// Runs `body` on its own thread and classifies the result.
///
/// Distinguishing `Panicked` from `Deadlocked` matters: a bare `recv_timeout`
/// error would report an assertion failure inside the scenario as a deadlock,
/// which is precisely the lie these tests are built to avoid.
fn run_scenario(body: impl FnOnce() + Send + 'static) -> Outcome {
    let (tx, rx) = std::sync::mpsc::channel();
    // Leaked deliberately on the deadlock path: the thread is wedged holding a
    // lock it will never release, which is the thing being demonstrated.
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(body));
        let _ = tx.send(result.map_err(|payload| {
            payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_owned())
        }));
    });
    match rx.recv_timeout(WAIT) {
        Ok(Ok(())) => Outcome::Completed,
        Ok(Err(msg)) => Outcome::Panicked(msg),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Outcome::Deadlocked,
        // `tx` is owned by the closure and only dropped when it finishes, and
        // the closure always sends first. Reaching this means the process
        // aborted the thread outright.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Outcome::Panicked("<thread aborted without sending>".to_owned())
        }
    }
}

/// The guard every scenario runs last: without it, a timeout would be
/// indistinguishable from "delivery never happened".
fn assert_callback_ran(flag: &AtomicBool) {
    std::thread::sleep(DELIVERY_GRACE);
    assert!(
        flag.load(Ordering::SeqCst),
        "the callback never ran, so this test proves nothing about re-entrancy"
    );
}

/// Dropping a `SampleMissListener` from inside the sample callback.
///
/// `publish_to` is separate from the subscribed key expression only so the
/// control test can aim the sample somewhere the subscriber does not match.
fn drop_miss_listener_in_callback(subscribe_to: &'static str, publish_to: &'static str) -> Outcome {
    run_scenario(move || {
        let session = zenoh::open(isolated_config()).wait().unwrap();

        let slot: Slot = Arc::new(Mutex::new(None));
        let in_callback = Arc::new(AtomicBool::new(false));

        let slot_cb = slot.clone();
        let flag = in_callback.clone();
        let sub = session
            .declare_subscriber(subscribe_to)
            .callback(move |_s: Sample| {
                flag.store(true, Ordering::SeqCst);
                // Re-entry: this `Drop` takes `statesref`, which the caller of
                // this very callback is holding.
                let taken = slot_cb.lock().unwrap().take();
                drop(taken);
            })
            .advanced()
            .wait()
            .unwrap();

        let listener = sub
            .sample_miss_listener()
            .callback(|_m: Miss| {})
            .wait()
            .unwrap();
        *slot.lock().unwrap() = Some(Box::new(listener));

        session.put(publish_to, "trigger").wait().unwrap();

        assert_callback_ran(&in_callback);
    })
}

/// Registering a `SampleMissListener` from inside the sample callback.
///
/// The subscriber is parked in a slot its own callback closes over, which is
/// what makes the re-entrant call reachable at all.
fn register_miss_listener_in_callback() -> Outcome {
    run_scenario(|| {
        let session = zenoh::open(isolated_config()).wait().unwrap();

        let sub_slot: SubSlot = Arc::new(Mutex::new(None));
        let holder: Slot = Arc::new(Mutex::new(None));
        let in_callback = Arc::new(AtomicBool::new(false));

        let sub_slot_cb = sub_slot.clone();
        let holder_cb = holder.clone();
        let flag = in_callback.clone();

        let sub = session
            .declare_subscriber("test/reentrancy/register")
            .callback(move |_s: Sample| {
                flag.store(true, Ordering::SeqCst);
                // Re-entry through the registration path rather than `Drop`:
                // `SampleMissListenerBuilder::wait` takes `statesref`.
                if let Some(sub) = sub_slot_cb.lock().unwrap().as_ref() {
                    let listener = sub
                        .sample_miss_listener()
                        .callback(|_m: Miss| {})
                        .wait()
                        .unwrap();
                    *holder_cb.lock().unwrap() = Some(Box::new(listener));
                }
            })
            .advanced()
            .wait()
            .unwrap();

        *sub_slot.lock().unwrap() = Some(sub);

        session
            .put("test/reentrancy/register", "trigger")
            .wait()
            .unwrap();

        assert_callback_ran(&in_callback);
    })
}

#[test]
fn dropping_a_miss_listener_inside_the_callback_is_safe() {
    let outcome = drop_miss_listener_in_callback("test/reentrancy/drop", "test/reentrancy/drop");
    assert!(
        matches!(outcome, Outcome::Completed),
        "`SampleMissListener::drop` takes `statesref`. This deadlocked until the \
         sample callback stopped being invoked under that guard; a `Deadlocked` \
         here means collect · release · call has been undone somewhere. Got \
         {outcome:?}."
    );
}

#[test]
fn registering_a_miss_listener_inside_the_callback_is_safe() {
    let outcome = register_miss_listener_in_callback();
    assert!(
        matches!(outcome, Outcome::Completed),
        "`SampleMissListenerBuilder::wait` takes `statesref`. This deadlocked \
         until the sample callback stopped being invoked under that guard. Got \
         {outcome:?}."
    );
}

/// Control. Publishes where the subscriber does not match, so the callback
/// cannot run and no lock is ever re-entered.
///
/// This is what makes the two tests above trustworthy. It proves the
/// `assert_callback_ran` guard is live — a scenario that never enters the
/// callback is reported as a *panic*, not silently as a deadlock — and with it
/// that [`run_scenario`] really does tell the two apart. Without this control,
/// the assertion above would be an assertion nobody has ever seen execute.
#[test]
fn the_callback_guard_fires_when_the_callback_never_runs() {
    // Its own key namespace, so a sibling test running concurrently cannot
    // deliver into it and falsify the premise.
    let outcome =
        drop_miss_listener_in_callback("test/reentrancy/control", "test/reentrancy/control-other");
    match outcome {
        Outcome::Panicked(msg) => assert!(
            msg.contains("the callback never ran"),
            "expected the in-callback guard to fire, got a different panic: {msg}"
        ),
        other => panic!(
            "expected the in-callback guard to fire when nothing is delivered, got {other:?}"
        ),
    }
}
