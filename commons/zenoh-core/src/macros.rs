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

// The three synchronous macros below wrap the guard they return in
// `zenoh_core::tracking::TrackedGuard`, which counts live guards per thread so
// that `tracking::assert_no_locks_held` can catch calls into user code made
// while a lock is held. The wrapper is `Deref`/`DerefMut`-transparent, and in
// release builds it is a `repr(transparent)` newtype with no `Drop` impl.
//
// The async macros further down are deliberately NOT wrapped: a task migrates
// threads across `.await`, so a thread-local count is wrong in both directions
// for a guard held across a suspension point. See the `tracking` module docs.

// This macro performs a standard lock on Mutex<T>
// For performance reasons, it first performs a try_lock() and,
// if it fails, it falls back on lock().unwrap()
#[macro_export]
macro_rules! zlock {
    ($var:expr) => {
        $crate::tracking::TrackedGuard::new_at(
            $var.lock().unwrap(),
            concat!(file!(), ":", line!()),
            $crate::tracking::LockKind::State,
        )
    };
}

// This macro performs a standard read on RwLock<T>
// For performance reasons, it first performs a try_read() and,
// if it fails, it falls back on read()
#[macro_export]
macro_rules! zread {
    ($var:expr) => {
        $crate::tracking::TrackedGuard::new_at(
            $var.read().unwrap(),
            concat!(file!(), ":", line!()),
            $crate::tracking::LockKind::State,
        )
    };
}

// This macro performs a standard write on RwLock<T>
// For performance reasons, it first performs a try_write() and,
// if it fails, it falls back on write()
#[macro_export]
macro_rules! zwrite {
    ($var:expr) => {
        $crate::tracking::TrackedGuard::new_at(
            $var.write().unwrap(),
            concat!(file!(), ":", line!()),
            $crate::tracking::LockKind::State,
        )
    };
}

// Like `zlock!`, but marks the lock as one held deliberately to serialise
// delivery rather than to guard state. `assert_no_locks_held` ignores these.
//
// Use it only where holding the lock across a call into user code is the
// intent — the transport RX channel lock, whose whole job is keeping multi-link
// delivery ordered. Using it to silence a report about a state lock defeats the
// check.
#[macro_export]
macro_rules! zlock_delivery {
    ($var:expr) => {
        $crate::tracking::TrackedGuard::new_at(
            $var.lock().unwrap(),
            concat!(file!(), ":", line!()),
            $crate::tracking::LockKind::DeliveryOrdering,
        )
    };
}

// This macro performs an async lock on Mutex<T>
#[macro_export]
macro_rules! zasynclock {
    ($var:expr) => {
        $var.lock().await
    };
}

// This macro performs an async read on RwLock<T>
// For performance reasons, it first performs a try_read() and,
// if it fails, it falls back on read().await
#[macro_export]
macro_rules! zasyncread {
    ($var:expr) => {
        if let Ok(g) = $var.try_read() {
            g
        } else {
            $var.read().await
        }
    };
}

// This macro performs an async write on RwLock<T>
// For performance reasons, it first performs a try_write() and,
// if it fails, it falls back on write().await
#[macro_export]
macro_rules! zasyncwrite {
    ($var:expr) => {
        if let Ok(g) = $var.try_write() {
            g
        } else {
            $var.write().await
        }
    };
}

// This macro returns &T from RwLock<Option<T>>
// This macro assumes that Option is always Some(T)
#[macro_export]
macro_rules! zasyncopt {
    ($var:expr) => {
        zasyncread!($var).as_ref().unwrap()
    };
}

// This macro performs an async send on Channel<T>
// For performance reasons, it first performs a try_send() and,
// if it fails, it falls back on send().await
#[macro_export]
macro_rules! zasyncsend {
    ($ch:expr, $var:expr) => {
        if $ch.try_send($var).is_err() {
            $ch.send($var).await;
        }
    };
}

// This macro performs an async recv on Channel<T>
// For performance reasons, it first performs a try_recv() and,
// if it fails, it falls back on recv().await
#[macro_export]
macro_rules! zasyncrecv {
    ($ch:expr) => {
        if let Ok(v) = $ch.try_recv() {
            Ok(v)
        } else {
            $ch.recv().await
        }
    };
}

// This macro allows to define some compile time configurable static constants
#[macro_export]
macro_rules! zconfigurable {
    ($(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $(#[$attr])* $crate::lazy_static!(static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e}
        };) ;
        $crate::zconfigurable!($($t)*);
    };
    ($(#[$attr:meta])* pub static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $(#[$attr])* $crate::lazy_static!(pub static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e}
        };) ;
        $crate::zconfigurable!($($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $(#[$attr])* $crate::lazy_static!(pub ($($vis)+) static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e}
        };) ;
        $crate::zconfigurable!($($t)*);
    };
    () => ()
}

// This macro is a shorthand for the conversion to u64
// This macro requires to previously import the following:
//   use std::convert::TryFrom;
#[macro_export]
macro_rules! to_u64 {
    ($val:expr) => {
        u64::try_from($val).unwrap_or_else(|_| {
            panic!(
                "Can not encode {} as u64 (max u64 value: {})",
                $val,
                u64::MAX
            )
        })
    };
}

// This macro allows to parse a string to the target type
#[macro_export]
macro_rules! zparse {
    ($str:expr) => {
        $str.parse().map_err(|_| {
            let e = zenoh_result::zerror!(
                "Failed to read configuration: {} is not a valid value",
                $str
            );
            tracing::warn!("{}", e);
            e
        })
    };
}

// This macro allows to parse a string to the target type
// No faili, but log the error and use default
#[macro_export]
macro_rules! zparse_default {
    ($str:expr, $default:expr) => {
        match $str.parse() {
            Ok(value) => value,
            Err(_) => {
                let e = zenoh_result::zerror!(
                    "Failed to read configuration: {} is not a valid value",
                    $str
                );
                tracing::warn!("{}", e);
                $default
            }
        }
    };
}

// This macro allows to do conditional compilation
#[macro_export]
macro_rules! zcondfeat {
    ($feature:literal, $yes:expr, $not:expr) => {{
        {
            #[cfg(feature = $feature)]
            {
                $yes
            }

            #[cfg(not(feature = $feature))]
            {
                $not
            }
        }
    }};
}

// This macro allows to timeout a feature
#[macro_export]
macro_rules! ztimeout {
    ($f:expr) => {
        tokio::time::timeout(TIMEOUT, ::core::future::IntoFuture::into_future($f))
            .await
            .unwrap()
    };
}
