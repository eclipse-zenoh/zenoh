//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

// This macro performs an async lock on Mutex<T>
// For performance reasons, it first performs a try_lock() and,
// if it fails, it falls back on lock().await
#[macro_export]
macro_rules! zasynclock {
    ($var:expr) => (
        if let Some(g) = $var.try_lock() { 
            g 
        } else { 
            $var.lock().await 
        }
    );
}

// This macro performs an async read on RwLock<T>
// For performance reasons, it first performs a try_read() and,
// if it fails, it falls back on read().await
#[macro_export]
macro_rules! zasyncread {
    ($var:expr) => (
        if let Some(g) = $var.try_read() { 
            g 
        } else { 
            $var.read().await 
        }
    );
}

// This macro performs an async write on RwLock<T>
// For performance reasons, it first performs a try_write() and,
// if it fails, it falls back on write().await
#[macro_export]
macro_rules! zasyncwrite {
    ($var:expr) => (
        if let Some(g) = $var.try_write() { 
            g 
        } else { 
            $var.write().await 
        }
    );
}

// This macro performs an async send on Channel<T>
// For performance reasons, it first performs a try_send() and,
// if it fails, it falls back on send().await
#[macro_export]
macro_rules! zasyncsend {
    ($ch:expr, $var:expr) => (
        if $ch.try_send($var).is_err() { 
            $ch.send($var).await;
        }
    );
}

// This macro performs an async recv on Channel<T>
// For performance reasons, it first performs a try_recv() and,
// if it fails, it falls back on recv().await
#[macro_export]
macro_rules! zasyncrecv {
    ($ch:expr) => (
        if let Ok(v) = $ch.try_recv() { 
            Ok(v)
        } else {
            $ch.recv().await
        }
    );
}

// This macro performs an upgrade on a weak pointer returning 
// a ZError with custom description in case of error
#[macro_export]
macro_rules! zweak {
    ($var:expr, $descr:expr) => (
        if let Some(inner) = $var.upgrade() { 
            inner
        } else {
            return zerror!(ZErrorKind::InvalidReference {
                descr: $descr.to_string()
            })
        }
    );
}

// This macro allows to define some compile time configurable static constants
#[macro_export]
macro_rules! zconfigurable {
    ($(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        lazy_static!($(#[$attr])* static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e} 
        };) ; 
        zconfigurable!($($t)*);
    };
    ($(#[$attr:meta])* pub static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        lazy_static!($(#[$attr])* pub static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e} 
        };) ; 
        zconfigurable!($($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        lazy_static!($(#[$attr])* pub ($($vis)+) static ref $N : $T = match option_env!(stringify!($N)) {
            Some(value) => {value.parse().unwrap()}
            None => {$e} 
        };) ; 
        zconfigurable!($($t)*);
    };
    () => ()
}

// This macro is a shorthand for the creation of a ZError
#[macro_export]
macro_rules! zerror {
    ($kind:expr) => (Err(ZError::new($kind, file!(), line!(), None)));
    ($kind:expr, $source:expr) => (Err(ZError::new($kind, file!(), line!(), Some(Box::new($source)))));
    ($kind:ident, $descr:expr, $source:expr) => (
        Err(ZError::new(ZErrorKind::$kind{descr:$descr}, file!(), line!(), Some(Box::new($source))));
    )
}

// This macro is a shorthand for the conversion of any Error into a ZError
#[macro_export]
macro_rules! to_zerror {
    ($kind:ident, $descr:expr) => (
        |e| { zerror!($kind, $descr, e) }
    )
}