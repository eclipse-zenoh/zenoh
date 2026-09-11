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

//! Re-entrancy tracking for Zenoh's synchronous lock macros.
//!
//! Calling user code — a subscriber callback, a query handler, a listener —
//! while holding a Zenoh lock is a latent self-deadlock: if that code re-enters
//! an API taking the same non-reentrant lock, one thread deadlocks against
//! itself, with no race and no timing dependency. The rule is
//! *collect · release · call*, and it is enforced today by review alone.
//!
//! This module makes it enforceable. Every synchronous acquisition in the
//! workspace funnels through [`zlock!`](crate::zlock), [`zread!`](crate::zread)
//! and [`zwrite!`](crate::zwrite), so wrapping the guard those macros return in
//! a newtype that maintains a per-thread count of live guards instruments the
//! whole codebase without touching a single call site. A call-out point then
//! asserts that count is zero via [`assert_no_locks_held`].
//!
//! # Coverage boundary
//!
//! **Only the three synchronous macros are tracked.** The asynchronous
//! `zasynclock!` / `zasyncread!` / `zasyncwrite!` macros are deliberately
//! **not** instrumented, and this is not an oversight — see
//! [the async section](#why-async-guards-are-not-tracked) below.
//!
//! Raw `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` calls that
//! bypass the macros are also uncovered. That gap is purely syntactic and can be
//! closed by a lint outside `zenoh-core`.
//!
//! # Known false positive: delivery-ordering locks
//!
//! The rule this enforces — "no lock held when calling out" — is stronger than
//! the property that actually matters, and it is worth knowing where the two
//! come apart before trusting a report.
//!
//! Two different jobs get done with a mutex here:
//!
//! * a **state lock**, guarding a data structure. Calling user code under one is
//!   a genuine hazard: re-entry from the callback self-deadlocks on one thread.
//!   This is the shape the check was built for.
//! * a **delivery-ordering lock**, held precisely *so that* deliveries are
//!   serialised. Holding it across user code is the whole point, and releasing
//!   it early trades a rare stall for out-of-order delivery.
//!
//! [`assert_no_locks_held`] cannot tell them apart, so it flags the second kind
//! too. The known instance is `zenoh-transport`'s per-priority RX channel mutex,
//! taken in `handle_frame` to validate the sequence number and then held across
//! the whole dispatch. That is deliberate: the channel is shared across a
//! transport's links, and the lock is what keeps reliable delivery ordered
//! between them. It is not a defect and should not be "fixed" to satisfy this
//! check.
//!
//! Note the corollary for anything this check pushes you to change: removing a
//! state lock from a delivery path may remove ordering that was being provided
//! incidentally. `zenoh-ext`'s `AdvancedSubscriber` had to grow an explicit
//! FIFO-and-single-deliverer to replace exactly that.
//!
//! # Why async guards are not tracked
//!
//! The counter is a `thread_local!`. An async task may be polled on one worker
//! thread, suspend at an `.await`, and resume on a different one. A guard held
//! across such a suspension point would therefore be *incremented* on one thread
//! and *decremented* on another — the first thread's count never falls (it
//! over-counts, producing false positives on unrelated work later scheduled
//! there) and the second underflows (it under-counts, producing false negatives
//! precisely where a real violation would show).
//!
//! A thread-local is not merely imprecise for async guards, it is wrong in both
//! directions, so instrumenting them with one would be worse than leaving them
//! out. Covering them correctly needs a *task*-local, which is runtime-specific
//! (`tokio::task_local!`) and would tie `zenoh-core` to a particular executor.
//! That trade is not made here; the boundary is documented instead.
//!
//! # Cost
//!
//! In release builds the counter field is `#[cfg]`-ed away entirely, leaving
//! [`TrackedGuard`] a `#[repr(transparent)]` newtype with no `Drop` impl — the
//! same code a bare guard compiles to. The newtype itself is present in *both*
//! profiles on purpose: gating the type as well would let a call site compile in
//! debug and fail in release.

use std::{
    fmt,
    ops::{Deref, DerefMut},
};

#[cfg(debug_assertions)]
mod counter {
    use std::cell::Cell;

    thread_local! {
        /// Number of live [`super::TrackedGuard`]s on this thread.
        static HELD: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn enter() {
        HELD.with(|h| h.set(h.get().saturating_add(1)));
    }

    pub(super) fn leave() {
        HELD.with(|h| h.set(h.get().saturating_sub(1)));
    }

    pub(super) fn held() -> usize {
        HELD.with(|h| h.get())
    }
}

/// Number of Zenoh synchronous lock guards currently live on this thread.
///
/// Always `0` when `debug_assertions` are off.
#[inline]
pub fn locks_held() -> usize {
    #[cfg(debug_assertions)]
    {
        counter::held()
    }
    #[cfg(not(debug_assertions))]
    {
        0
    }
}

/// What [`zlock!`](crate::zlock) returns.
///
/// Needed wherever a signature names the guard type — `Deref` transparency
/// covers *using* a guard but not *declaring* one.
pub type TrackedMutexGuard<'a, T> = TrackedGuard<std::sync::MutexGuard<'a, T>>;

/// What [`zread!`](crate::zread) returns.
pub type TrackedReadGuard<'a, T> = TrackedGuard<std::sync::RwLockReadGuard<'a, T>>;

/// What [`zwrite!`](crate::zwrite) returns.
pub type TrackedWriteGuard<'a, T> = TrackedGuard<std::sync::RwLockWriteGuard<'a, T>>;

/// RAII counter half of [`TrackedGuard`], split out so its `Drop` ordering
/// relative to the lock guard is expressible as field order.
#[cfg(debug_assertions)]
#[derive(Debug)]
pub struct CounterGuard {
    /// `MutexGuard` is `!Send + Sync`, which is exactly the marker wanted here:
    /// the count belongs to the thread that incremented it, so the guard must
    /// not cross threads, but sharing a `&` reference remains harmless.
    _not_send: std::marker::PhantomData<std::sync::MutexGuard<'static, ()>>,
}

#[cfg(debug_assertions)]
impl CounterGuard {
    #[inline]
    #[allow(clippy::new_without_default)] // acquiring a count is not a default
    pub fn new() -> Self {
        counter::enter();
        Self {
            _not_send: std::marker::PhantomData,
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for CounterGuard {
    #[inline]
    fn drop(&mut self) {
        counter::leave();
    }
}

/// A lock guard that counts itself while it is alive.
///
/// Transparently substitutable for the guard it wraps via [`Deref`] and
/// [`DerefMut`], so `let mut s = zwrite!(x)`, `&mut *zlock!(x)` and
/// `zread!(x).field` all keep working unchanged.
///
/// # Field order is load-bearing
///
/// Rust drops struct fields in declaration order, so `inner` — declared
/// **first** — releases the lock *before* the counter falls. That direction is
/// the safe one: it leaves a window in which the count exceeds the number of
/// locks actually held, which can only ever produce a false *positive*.
///
/// Reversed, the counter would reach zero while the lock was still held, and
/// [`assert_no_locks_held`] would pass on a genuine violation — a false
/// negative, i.e. the check silently failing to check. The invariant to preserve
/// is `locks_held() >= |locks actually held|` at every instant.
///
/// `tests::field_order_is_load_bearing` executes both orders and shows only one
/// upholds it. (A C++ port needs the opposite declaration order, since C++
/// destroys members in reverse declaration order.)
#[cfg_attr(not(debug_assertions), repr(transparent))]
pub struct TrackedGuard<G> {
    // MUST stay first. See the type's documentation.
    inner: G,
    #[cfg(debug_assertions)]
    _counter: CounterGuard,
}

impl<G> TrackedGuard<G> {
    /// Wraps `inner`, counting it as held for as long as the wrapper lives.
    #[inline]
    pub fn new(inner: G) -> Self {
        Self {
            inner,
            #[cfg(debug_assertions)]
            _counter: CounterGuard::new(),
        }
    }

    /// Unwraps to the underlying guard, dropping the count.
    ///
    /// This *disables* tracking for the returned guard, so the lock stays held
    /// while the counter no longer knows about it. Use only where a concrete
    /// guard type is unavoidable, and prefer [`Deref`] everywhere else.
    #[inline]
    pub fn into_inner(self) -> G {
        #[cfg(debug_assertions)]
        {
            // A field holding a `Drop` type blocks a plain destructure, so move
            // both fields out through a `ManuallyDrop`.
            let this = std::mem::ManuallyDrop::new(self);
            // SAFETY: `this` is never dropped nor otherwise read again, and each
            // field is read exactly once.
            unsafe {
                let inner = std::ptr::read(&this.inner);
                let counter = std::ptr::read(&this._counter);
                drop(counter);
                inner
            }
        }
        #[cfg(not(debug_assertions))]
        {
            self.inner
        }
    }
}

impl<'a, T> TrackedGuard<std::sync::MutexGuard<'a, T>> {
    /// [`Condvar::wait`](std::sync::Condvar::wait), keeping the guard tracked.
    ///
    /// `Condvar` takes a concrete `MutexGuard` by value, one of the few places
    /// where [`Deref`] transparency is not enough. Doing this by hand —
    /// [`into_inner`](Self::into_inner), wait, re-wrap — would drop the count to
    /// zero while the lock is still held on either side of the wait, reopening
    /// the false-negative window the field order exists to close. Holding a
    /// second count across the swap keeps it from ever reaching zero.
    ///
    /// The count stays up for the duration of the wait, during which `Condvar`
    /// has actually released the mutex. That direction is the safe one: it
    /// over-counts, and over-counting can only produce a false positive.
    pub fn wait_on(
        self,
        condvar: &std::sync::Condvar,
    ) -> std::sync::LockResult<TrackedGuard<std::sync::MutexGuard<'a, T>>> {
        #[cfg(debug_assertions)]
        let _keep_counted = CounterGuard::new();
        let raw = self.into_inner();
        match condvar.wait(raw) {
            Ok(guard) => Ok(Self::new(guard)),
            Err(poisoned) => Err(std::sync::PoisonError::new(Self::new(
                poisoned.into_inner(),
            ))),
        }
    }
}

impl<G: Deref> Deref for TrackedGuard<G> {
    type Target = G::Target;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<G: DerefMut> DerefMut for TrackedGuard<G> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<G: fmt::Debug> fmt::Debug for TrackedGuard<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl<G: fmt::Display> fmt::Display for TrackedGuard<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

/// Panics if any Zenoh synchronous lock guard is live on this thread.
///
/// Place immediately before calling out to code Zenoh does not control — a user
/// callback, a handler, a listener. `site` names the call-out for the message.
///
/// A no-op when `debug_assertions` are off.
///
/// Remember the [coverage boundary](self#coverage-boundary): async guards and
/// raw acquisitions are invisible here, so a silent pass is evidence only about
/// the macro-acquired synchronous locks.
#[inline]
#[track_caller]
pub fn assert_no_locks_held(site: &str) {
    #[cfg(debug_assertions)]
    {
        let held = counter::held();
        assert!(
            held == 0,
            "re-entrancy hazard at {site}: {held} Zenoh lock guard(s) still held \
             while calling out to code Zenoh does not control. If that code \
             re-enters an API taking the same non-reentrant lock, this thread \
             deadlocks against itself. Apply collect · release · call: gather \
             what is needed, drop the guard, then call."
        );
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = site;
    }
}

#[cfg(test)]
mod tests {
    #[cfg(debug_assertions)]
    use std::cell::Cell;
    use std::sync::{Mutex, RwLock};

    use super::*;

    /// Structural half of the zero-cost claim: the wrapper adds no bytes. The
    /// timing half is the benchmark, which cannot live in a unit test.
    #[test]
    fn the_wrapper_adds_no_size() {
        assert_eq!(
            std::mem::size_of::<TrackedGuard<std::sync::MutexGuard<'_, u64>>>(),
            std::mem::size_of::<std::sync::MutexGuard<'_, u64>>(),
        );
    }

    /// In release the counter is gone, so the tripwire must be inert even with
    /// guards held — otherwise the gating is wrong.
    #[test]
    #[cfg(not(debug_assertions))]
    fn the_tripwire_is_inert_in_release() {
        let m = Mutex::new(());
        let _g = TrackedGuard::new(m.lock().unwrap());
        assert_eq!(locks_held(), 0);
        assert_no_locks_held("test/release-inert");
    }

    #[test]
    fn guard_derefs_like_the_guard_it_wraps() {
        let m = Mutex::new(vec![1, 2, 3]);
        {
            let mut g = TrackedGuard::new(m.lock().unwrap());
            g.push(4); // DerefMut, method call
            assert_eq!(g.len(), 4); // Deref, method call
            assert_eq!((*g)[0], 1); // explicit deref
            let r: &mut Vec<i32> = &mut g; // &mut *guard idiom
            r.push(5);
        }
        assert_eq!(&*m.lock().unwrap(), &[1, 2, 3, 4, 5]);

        let rw = RwLock::new(7u8);
        assert_eq!(*TrackedGuard::new(rw.read().unwrap()), 7);
        *TrackedGuard::new(rw.write().unwrap()) = 9;
        assert_eq!(*rw.read().unwrap(), 9);
    }

    #[test]
    fn counting_is_balanced_and_nests() {
        let a = Mutex::new(0);
        let b = Mutex::new(0);
        assert_eq!(locks_held(), 0);
        let ga = TrackedGuard::new(a.lock().unwrap());
        assert_eq!(locks_held(), usize::from(cfg!(debug_assertions)));
        {
            let _gb = TrackedGuard::new(b.lock().unwrap());
            assert_eq!(locks_held(), 2 * usize::from(cfg!(debug_assertions)));
        }
        assert_eq!(locks_held(), usize::from(cfg!(debug_assertions)));
        drop(ga);
        assert_eq!(locks_held(), 0);
    }

    #[test]
    fn assertion_is_silent_with_no_guard_held() {
        assert_no_locks_held("test/no-guard");
    }

    /// The detector must be *shown* to fire. A check that has never printed a
    /// failure is unvalidated.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "re-entrancy hazard at test/under-guard")]
    fn assertion_fires_under_a_live_guard() {
        let m = Mutex::new(());
        let _g = TrackedGuard::new(m.lock().unwrap());
        assert_no_locks_held("test/under-guard");
    }

    /// ...and must be shown to fire for a guard acquired through the macro, not
    /// only one constructed by hand.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "re-entrancy hazard at test/via-macro")]
    fn assertion_fires_under_a_macro_acquired_guard() {
        let m = Mutex::new(0u8);
        let _g = crate::zlock!(m);
        assert_no_locks_held("test/via-macro");
    }

    #[test]
    fn macros_still_read_and_write() {
        let m = Mutex::new(1u8);
        assert_eq!(*crate::zlock!(m), 1);
        let rw = RwLock::new(1u8);
        *crate::zwrite!(rw) = 2;
        assert_eq!(*crate::zread!(rw), 2);
    }

    #[test]
    fn into_inner_releases_the_count_and_keeps_the_lock() {
        let m = Mutex::new(5u8);
        let g = TrackedGuard::new(m.lock().unwrap());
        assert_eq!(locks_held(), usize::from(cfg!(debug_assertions)));
        let raw = g.into_inner();
        assert_eq!(locks_held(), 0, "count must be released");
        assert_eq!(*raw, 5, "but the lock must still be held");
        assert!(m.try_lock().is_err(), "still locked while `raw` lives");
        drop(raw);
        assert!(m.try_lock().is_ok());
    }

    // ---- field order -------------------------------------------------------
    //
    // Gated as a block: `CounterGuard` itself only exists under
    // `debug_assertions`, so the scaffolding below cannot be compiled in
    // release even though none of it would run there.

    #[cfg(debug_assertions)]
    thread_local! {
        /// Count observed at the instant the stand-in lock is released.
        static OBSERVED_AT_RELEASE: Cell<Option<usize>> = const { Cell::new(None) };
    }

    /// Stands in for a real lock guard: its `Drop` is the moment the lock is
    /// released, so it samples the counter exactly then.
    #[cfg(debug_assertions)]
    struct ProbeGuard;

    #[cfg(debug_assertions)]
    impl Drop for ProbeGuard {
        fn drop(&mut self) {
            OBSERVED_AT_RELEASE.with(|o| o.set(Some(locks_held())));
        }
    }

    /// The safe order, mirroring [`TrackedGuard`]: lock released first.
    #[cfg(debug_assertions)]
    struct GoodOrder {
        _inner: ProbeGuard,
        _counter: CounterGuard,
    }

    /// The unsafe order: counter falls first.
    #[cfg(debug_assertions)]
    struct BadOrder {
        _counter: CounterGuard,
        _inner: ProbeGuard,
    }

    #[cfg(debug_assertions)]
    fn observe(build: impl FnOnce()) -> usize {
        OBSERVED_AT_RELEASE.with(|o| o.set(None));
        build();
        OBSERVED_AT_RELEASE
            .with(|o| o.get())
            .expect("the probe guard must have been dropped")
    }

    /// Executes both field orders and shows only one upholds
    /// `locks_held() >= |locks actually held|` at the instant of release.
    #[test]
    #[cfg(debug_assertions)]
    fn field_order_is_load_bearing() {
        let good = observe(|| {
            drop(GoodOrder {
                _inner: ProbeGuard,
                _counter: CounterGuard::new(),
            })
        });
        assert_eq!(
            good, 1,
            "inner-first: the count must still be non-zero while the lock is \
             being released, so the check cannot pass on a held lock"
        );

        let bad = observe(|| {
            drop(BadOrder {
                _counter: CounterGuard::new(),
                _inner: ProbeGuard,
            })
        });
        assert_eq!(
            bad, 0,
            "counter-first: the count reads 0 while the lock is still held — \
             this is the false-negative window `TrackedGuard`'s field order \
             exists to close"
        );

        assert_ne!(good, bad, "the two orders must be distinguishable");
        assert_eq!(locks_held(), 0, "both orders must still balance");
    }

    /// The same window, stated as the property that actually matters: a
    /// re-entrancy check running at the instant of release passes under the bad
    /// order and fails under the good one.
    #[test]
    #[cfg(debug_assertions)]
    fn bad_field_order_lets_the_check_pass_on_a_held_lock() {
        let good_would_fire = observe(|| {
            drop(GoodOrder {
                _inner: ProbeGuard,
                _counter: CounterGuard::new(),
            })
        }) > 0;
        let bad_would_fire = observe(|| {
            drop(BadOrder {
                _counter: CounterGuard::new(),
                _inner: ProbeGuard,
            })
        }) > 0;
        assert!(good_would_fire, "the safe order keeps the tripwire armed");
        assert!(
            !bad_would_fire,
            "the reversed order disarms the tripwire while the lock is held"
        );
    }
}
