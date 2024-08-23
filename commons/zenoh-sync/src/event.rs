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
    sync::{
        atomic::{AtomicU16, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

// Return types
pub struct EventClosed;

impl std::fmt::Display for EventClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Debug for EventClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

/// Creates a new condition variable with a given capacity.
/// The capacity indicates the maximum number of tasks that
/// may be waiting on the condition.
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
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
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
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
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
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
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
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
            }

            // Start listening for events.
            let listener = self.0.event.listen();

            // Check the flag again after creating the listener.
            match self.0.check() {
                EventCheck::Ok => break,
                EventCheck::Err => return Err(EventClosed),
                EventCheck::Unset => {}
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
