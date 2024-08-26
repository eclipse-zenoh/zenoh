//
// Copyright (c) 2023 ZettaScale Technology
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
use event_listener::{Event as EventLib, Listener};
use std::{
    fmt,
    sync::{
        atomic::{AtomicU16, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

// Return types
pub struct EventClosed;

impl fmt::Display for EventClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Debug for EventClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Event Closed")
    }
}

impl std::error::Error for EventClosed {}

#[repr(u8)]
pub enum WaitDeadline {
    Event,
    Deadline,
}

#[repr(u8)]
pub enum WaitTimeout {
    Event,
    Timeout,
}

/// This is a Event Variable similar to that provided by POSIX.
/// As for POSIX condition variables, this assumes that a mutex is
/// properly used to coordinate behaviour. In other terms there should
/// not be race condition on [notify_one](Event::notify_one) or
/// [notify_all](Event::notify_all).
///
struct EventInner {
    event: EventLib,
    flag: AtomicU8,
    notifiers: AtomicU16,
    waiters: AtomicU16,
}

const UNSET: u8 = 0;
const OK: u8 = 1;
const ERR: u8 = 1 << 1;

#[repr(u8)]
enum EventCheck {
    Unset = UNSET,
    Ok = OK,
    Err = ERR,
}

#[repr(u8)]
enum EventSet {
    Ok = OK,
    Err = ERR,
}

impl EventInner {
    fn check(&self) -> EventCheck {
        let f = self.flag.fetch_and(!OK, Ordering::SeqCst);
        if f & ERR != 0 {
            return EventCheck::Err;
        }
        if f == OK {
            return EventCheck::Ok;
        }
        EventCheck::Unset
    }

    fn set(&self) -> EventSet {
        let f = self.flag.fetch_or(OK, Ordering::SeqCst);
        if f & ERR != 0 {
            return EventSet::Err;
        }
        EventSet::Ok
    }

    fn err(&self) {
        self.flag.store(ERR, Ordering::SeqCst);
    }
}

#[repr(transparent)]
pub struct Notifier(Arc<EventInner>);

impl Clone for Notifier {
    fn clone(&self) -> Self {
        let n = self.0.notifiers.fetch_add(1, Ordering::SeqCst);
        // Panic on overflow
        assert!(n != 0);
        Self(self.0.clone())
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        let n = self.0.notifiers.fetch_sub(1, Ordering::SeqCst);
        if n == 1 {
            // The last Notifier has been dropped, close the event and notify everyone
            self.0.err();
            self.0.event.notify(usize::MAX);
        }
    }
}

#[repr(transparent)]
pub struct Waiter(Arc<EventInner>);

impl Clone for Waiter {
    fn clone(&self) -> Self {
        let n = self.0.waiters.fetch_add(1, Ordering::Relaxed);
        // Panic on overflow
        assert!(n != 0);
        Self(self.0.clone())
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        let n = self.0.waiters.fetch_sub(1, Ordering::SeqCst);
        if n == 1 {
            // The last Waiter has been dropped, close the event
            self.0.err();
        }
    }
}

/// Creates a new lock-free event variable. Every time a [`Notifier`] calls ['Notifier::notify`], one [`Waiter`] will be waken-up.
/// If no waiter is waiting when the `notify` is called, the notification will not be lost. That means the next waiter will return
/// immediately when calling `wait`.
pub fn new() -> (Notifier, Waiter) {
    let inner = Arc::new(EventInner {
        event: EventLib::new(),
        flag: AtomicU8::new(UNSET),
        notifiers: AtomicU16::new(1),
        waiters: AtomicU16::new(1),
    });
    (Notifier(inner.clone()), Waiter(inner))
}

impl Waiter {
    /// Waits for the condition to be notified
    #[inline]
    pub async fn wait_async(&self) -> Result<(), EventClosed> {
        // Wait until the flag is set.
        loop {
            // Check the flag.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Wait for a notification and continue the loop.
            listener.await;
        }

        Ok(())
    }

    /// Waits for the condition to be notified
    #[inline]
    pub fn wait(&self) -> Result<(), EventClosed> {
        // Wait until the flag is set.
        loop {
            // Check the flag.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Wait for a notification and continue the loop.
            listener.wait();
        }

        Ok(())
    }

    /// Waits for the condition to be notified
    #[inline]
    pub fn wait_deadline(&self, deadline: Instant) -> Result<WaitDeadline, EventClosed> {
        // Wait until the flag is set.
        loop {
            // Check the flag.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Wait for a notification and continue the loop.
            if listener.wait_deadline(deadline).is_none() {
                return Ok(WaitDeadline::Deadline);
            }
        }

        Ok(WaitDeadline::Event)
    }

    /// Waits for the condition to be notified
    #[inline]
    pub fn wait_timeout(&self, timeout: Duration) -> Result<WaitTimeout, EventClosed> {
        // Wait until the flag is set.
        loop {
            // Check the flag.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Unset => {}
                EventCheck::Err => return Err(EventClosed),
            }

            // Wait for a notification and continue the loop.
            if listener.wait_timeout(timeout).is_none() {
                return Ok(WaitTimeout::Timeout);
            }
        }

        Ok(WaitTimeout::Event)
    }
}

impl Notifier {
    /// Notifies one pending listener
    #[inline]
    pub fn notify(&self) -> Result<(), EventClosed> {
        // Set the flag.
        match self.0.set() {
            EventSet::Ok => {
                self.0.event.notify_additional_relaxed(1);
                Ok(())
            }
            EventSet::Err => Err(EventClosed),
        }
    }
}

mod tests {
    #[test]
    fn event_steps() {
        use crate::{EventClosed, WaitTimeout};
        use std::{
            sync::{Arc, Barrier},
            time::Duration,
        };

        let barrier = Arc::new(Barrier::new(2));

        let (notifier, waiter) = super::new();

        let tslot = Duration::from_secs(1);

        let bs = barrier.clone();
        let s = std::thread::spawn(move || {
            // 1 - Wait one notification
            match waiter.wait_timeout(tslot) {
                Ok(WaitTimeout::Event) => {}
                Ok(WaitTimeout::Timeout) => panic!("Timeout {:#?}", tslot),
                Err(EventClosed) => panic!("Event closed"),
            }

            bs.wait();

            // 2 - Being notified twice but waiting only once
            bs.wait();

            match waiter.wait_timeout(tslot) {
                Ok(WaitTimeout::Event) => {}
                Ok(WaitTimeout::Timeout) => panic!("Timeout {:#?}", tslot),
                Err(EventClosed) => panic!("Event closed"),
            }

            match waiter.wait_timeout(tslot) {
                Ok(WaitTimeout::Event) => panic!("Event Ok but it should be Timeout"),
                Ok(WaitTimeout::Timeout) => {}
                Err(EventClosed) => panic!("Event closed"),
            }

            bs.wait();

            // 3 - Notifier has been dropped
            bs.wait();

            waiter.wait().unwrap_err();

            bs.wait();
        });

        let bp = barrier.clone();
        let p = std::thread::spawn(move || {
            // 1 - Notify once
            notifier.notify().unwrap();

            bp.wait();

            // 2 - Notify twice
            notifier.notify().unwrap();
            notifier.notify().unwrap();

            bp.wait();
            bp.wait();

            // 3 - Drop notifier yielding an error in the waiter
            drop(notifier);

            bp.wait();
            bp.wait();
        });

        s.join().unwrap();
        p.join().unwrap();
    }

    #[test]
    fn event_loop() {
        use std::{
            sync::atomic::{AtomicUsize, Ordering},
            time::{Duration, Instant},
        };

        const N: usize = 1_000;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (notifier, waiter) = super::new();

        let s = std::thread::spawn(move || {
            for _ in 0..N {
                waiter.wait().unwrap();
                COUNTER.fetch_add(1, Ordering::Relaxed);
            }
        });
        let p = std::thread::spawn(move || {
            for _ in 0..N {
                notifier.notify().unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
        });

        let start = Instant::now();
        let tout = Duration::from_secs(60);
        loop {
            let n = COUNTER.load(Ordering::Relaxed);
            if n == N {
                break;
            }
            if start.elapsed() > tout {
                panic!("Timeout {:#?}. Counter: {n}/{N}", tout);
            }

            std::thread::sleep(Duration::from_secs(1));
        }

        s.join().unwrap();
        p.join().unwrap();
    }
}
