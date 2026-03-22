//
// Copyright (c) 2024 ZettaScale Technology
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
use std::{
    fmt,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

// Error types
const WAIT_ERR_STR: &str = "No notifier available";
pub struct WaitError;

impl fmt::Display for WaitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for WaitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(WAIT_ERR_STR)
    }
}

impl std::error::Error for WaitError {}

#[repr(u8)]
pub enum WaitDeadlineError {
    Deadline,
    WaitError,
}

impl fmt::Display for WaitDeadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for WaitDeadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deadline => f.write_str("Deadline reached"),
            Self::WaitError => f.write_str(WAIT_ERR_STR),
        }
    }
}

impl std::error::Error for WaitDeadlineError {}

#[repr(u8)]
pub enum WaitTimeoutError {
    Timeout,
    WaitError,
}

impl fmt::Display for WaitTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for WaitTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("Timeout expired"),
            Self::WaitError => f.write_str(WAIT_ERR_STR),
        }
    }
}

impl std::error::Error for WaitTimeoutError {}

const NOTIFY_ERR_STR: &str = "No waiter available";
pub struct NotifyError;

impl fmt::Display for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(NOTIFY_ERR_STR)
    }
}

impl std::error::Error for NotifyError {}

// State values stored inside the mutex.
const UNSET: u8 = 0;
const OK: u8 = 1;
const ERR: u8 = 2;

// Replace event_listener::Event + AtomicU8 with std Mutex<u8> + Condvar.
// This eliminates per-wait EventListener allocation and intrusive-list
// register/deregister cost on the transport pipeline hot path.
struct EventInner {
    state: Mutex<u8>,
    cv: Condvar,
    notifiers: AtomicU16,
    waiters: AtomicU16,
}

/// Creates a new event variable. Every time a [`Notifier`] calls
/// [`Notifier::notify`], one [`Waiter`] will be woken up. Notifications are
/// sticky: if no waiter is blocking when `notify` is called, the next `wait`
/// call returns immediately.
pub fn new() -> (Notifier, Waiter) {
    let inner = Arc::new(EventInner {
        state: Mutex::new(UNSET),
        cv: Condvar::new(),
        notifiers: AtomicU16::new(1),
        waiters: AtomicU16::new(1),
    });
    (Notifier(inner.clone()), Waiter(inner))
}

/// A [`Notifier`] wakes up one [`Waiter`].
#[repr(transparent)]
pub struct Notifier(Arc<EventInner>);

impl Notifier {
    #[inline]
    pub fn notify(&self) -> Result<(), NotifyError> {
        let mut state = self.0.state.lock().unwrap();
        if *state == ERR {
            return Err(NotifyError);
        }
        *state = OK;
        self.0.cv.notify_one();
        Ok(())
    }
}

impl Clone for Notifier {
    fn clone(&self) -> Self {
        let n = self.0.notifiers.fetch_add(1, Ordering::SeqCst);
        assert!(n != 0);
        Self(self.0.clone())
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        let n = self.0.notifiers.fetch_sub(1, Ordering::SeqCst);
        if n == 1 {
            // Last notifier dropped — wake all waiters with error.
            let mut state = self.0.state.lock().unwrap();
            *state = ERR;
            self.0.cv.notify_all();
        }
    }
}

#[repr(transparent)]
pub struct Waiter(Arc<EventInner>);

impl Waiter {
    #[inline]
    pub fn wait(&self) -> Result<(), WaitError> {
        let mut state = self.0.state.lock().unwrap();
        loop {
            match *state {
                OK => { *state = UNSET; return Ok(()); }
                ERR => return Err(WaitError),
                _ => { state = self.0.cv.wait(state).unwrap(); }
            }
        }
    }

    #[inline]
    pub async fn wait_async(&self) -> Result<(), WaitError> {
        let waiter = self.clone();
        tokio::task::spawn_blocking(move || waiter.wait())
            .await
            .unwrap_or(Err(WaitError))
    }

    #[inline]
    pub fn wait_deadline(&self, deadline: Instant) -> Result<(), WaitDeadlineError> {
        let mut state = self.0.state.lock().unwrap();
        loop {
            match *state {
                OK => { *state = UNSET; return Ok(()); }
                ERR => return Err(WaitDeadlineError::WaitError),
                _ => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(WaitDeadlineError::Deadline);
                    }
                    let (new_state, timeout) = self.0.cv
                        .wait_timeout(state, deadline - now)
                        .unwrap();
                    state = new_state;
                    if timeout.timed_out() {
                        return match *state {
                            OK => { *state = UNSET; Ok(()) }
                            ERR => Err(WaitDeadlineError::WaitError),
                            _ => Err(WaitDeadlineError::Deadline),
                        };
                    }
                }
            }
        }
    }

    #[inline]
    pub fn wait_timeout(&self, timeout: Duration) -> Result<(), WaitTimeoutError> {
        match self.wait_deadline(Instant::now() + timeout) {
            Ok(()) => Ok(()),
            Err(WaitDeadlineError::Deadline) => Err(WaitTimeoutError::Timeout),
            Err(WaitDeadlineError::WaitError) => Err(WaitTimeoutError::WaitError),
        }
    }
}

impl Clone for Waiter {
    fn clone(&self) -> Self {
        let n = self.0.waiters.fetch_add(1, Ordering::Relaxed);
        assert!(n != 0);
        Self(self.0.clone())
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        let n = self.0.waiters.fetch_sub(1, Ordering::SeqCst);
        if n == 1 {
            let mut state = self.0.state.lock().unwrap();
            *state = ERR;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn event_timeout() {
        use std::{
            sync::{Arc, Barrier},
            time::Duration,
        };
        use crate::WaitTimeoutError;

        let barrier = Arc::new(Barrier::new(2));
        let (notifier, waiter) = super::new();
        let tslot = Duration::from_secs(1);

        let bs = barrier.clone();
        let s = std::thread::spawn(move || {
            match waiter.wait_timeout(tslot) {
                Ok(()) => {}
                Err(WaitTimeoutError::Timeout) => panic!("Timeout {tslot:#?}"),
                Err(WaitTimeoutError::WaitError) => panic!("Event closed"),
            }
            bs.wait();
            bs.wait();
            match waiter.wait_timeout(tslot) {
                Ok(()) => {}
                Err(WaitTimeoutError::Timeout) => panic!("Timeout {tslot:#?}"),
                Err(WaitTimeoutError::WaitError) => panic!("Event closed"),
            }
            match waiter.wait_timeout(tslot) {
                Ok(()) => panic!("Event Ok but it should be Timeout"),
                Err(WaitTimeoutError::Timeout) => {}
                Err(WaitTimeoutError::WaitError) => panic!("Event closed"),
            }
            bs.wait();
            bs.wait();
            waiter.wait().unwrap_err();
            bs.wait();
        });

        let bp = barrier.clone();
        let p = std::thread::spawn(move || {
            notifier.notify().unwrap();
            bp.wait();
            notifier.notify().unwrap();
            notifier.notify().unwrap();
            bp.wait();
            bp.wait();
            drop(notifier);
            bp.wait();
            bp.wait();
        });

        s.join().unwrap();
        p.join().unwrap();
    }

    #[test]
    fn event_deadline() {
        use std::{
            sync::{Arc, Barrier},
            time::{Duration, Instant},
        };
        use crate::WaitDeadlineError;

        let barrier = Arc::new(Barrier::new(2));
        let (notifier, waiter) = super::new();
        let tslot = Duration::from_secs(1);

        let bs = barrier.clone();
        let s = std::thread::spawn(move || {
            match waiter.wait_deadline(Instant::now() + tslot) {
                Ok(()) => {}
                Err(WaitDeadlineError::Deadline) => panic!("Timeout {tslot:#?}"),
                Err(WaitDeadlineError::WaitError) => panic!("Event closed"),
            }
            bs.wait();
            bs.wait();
            match waiter.wait_deadline(Instant::now() + tslot) {
                Ok(()) => {}
                Err(WaitDeadlineError::Deadline) => panic!("Timeout {tslot:#?}"),
                Err(WaitDeadlineError::WaitError) => panic!("Event closed"),
            }
            match waiter.wait_deadline(Instant::now() + tslot) {
                Ok(()) => panic!("Event Ok but it should be Timeout"),
                Err(WaitDeadlineError::Deadline) => {}
                Err(WaitDeadlineError::WaitError) => panic!("Event closed"),
            }
            bs.wait();
            bs.wait();
            waiter.wait().unwrap_err();
            bs.wait();
        });

        let bp = barrier.clone();
        let p = std::thread::spawn(move || {
            notifier.notify().unwrap();
            bp.wait();
            notifier.notify().unwrap();
            notifier.notify().unwrap();
            bp.wait();
            bp.wait();
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
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc, Barrier,
            },
            time::{Duration, Instant},
        };

        const N: usize = 1_000;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (notifier, waiter) = super::new();
        let barrier = Arc::new(Barrier::new(2));

        let bs = barrier.clone();
        let s = std::thread::spawn(move || {
            for _ in 0..N {
                waiter.wait().unwrap();
                COUNTER.fetch_add(1, Ordering::Relaxed);
                bs.wait();
            }
        });
        let p = std::thread::spawn(move || {
            for _ in 0..N {
                notifier.notify().unwrap();
                barrier.wait();
            }
        });

        let start = Instant::now();
        let tout = Duration::from_secs(60);
        loop {
            let n = COUNTER.load(Ordering::Relaxed);
            if n == N { break; }
            if start.elapsed() > tout {
                panic!("Timeout {tout:#?}. Counter: {n}/{N}");
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        s.join().unwrap();
        p.join().unwrap();
    }

    #[test]
    fn event_multiple() {
        use std::{
            sync::atomic::{AtomicUsize, Ordering},
            time::{Duration, Instant},
        };

        const N: usize = 1_000;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (notifier, waiter) = super::new();

        let w1 = waiter.clone();
        let s1 = std::thread::spawn(move || {
            let mut n = 0;
            while COUNTER.fetch_add(1, Ordering::Relaxed) < N - 2 {
                w1.wait().unwrap();
                n += 1;
            }
            println!("S1: {n}");
        });
        let s2 = std::thread::spawn(move || {
            let mut n = 0;
            while COUNTER.fetch_add(1, Ordering::Relaxed) < N - 2 {
                waiter.wait().unwrap();
                n += 1;
            }
            println!("S2: {n}");
        });

        let n1 = notifier.clone();
        let p1 = std::thread::spawn(move || {
            let mut n = 0;
            while COUNTER.load(Ordering::Relaxed) < N {
                n1.notify().unwrap();
                n += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            println!("P1: {n}");
        });
        let p2 = std::thread::spawn(move || {
            let mut n = 0;
            while COUNTER.load(Ordering::Relaxed) < N {
                notifier.notify().unwrap();
                n += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            println!("P2: {n}");
        });

        std::thread::spawn(move || {
            let start = Instant::now();
            let tout = Duration::from_secs(60);
            loop {
                let n = COUNTER.load(Ordering::Relaxed);
                if n == N { break; }
                if start.elapsed() > tout {
                    panic!("Timeout {tout:#?}. Counter: {n}/{N}");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });

        p1.join().unwrap();
        p2.join().unwrap();
        s1.join().unwrap();
        s2.join().unwrap();
    }
}
