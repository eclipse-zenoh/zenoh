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

// This macro performs a standard lock on Mutex<T>
// For performance reasons, it first performs a try_lock() and,
// if it fails, it falls back on lock().unwrap()
#[macro_export]
macro_rules! zlock {
    ($var:expr) => {
        match $var.try_lock() {
            Ok(guard) => guard,
            Err(_) => $var.lock().unwrap(),
        }
    };
}

// This macro performs a standard read on RwLock<T>
// For performance reasons, it first performs a try_read() and,
// if it fails, it falls back on read()
#[macro_export]
macro_rules! zread {
    ($var:expr) => {
        match $var.try_read() {
            Ok(guard) => guard,
            Err(_) => $var.read().unwrap(),
        }
    };
}

// This macro performs a standard write on RwLock<T>
// For performance reasons, it first performs a try_write() and,
// if it fails, it falls back on write()
#[macro_export]
macro_rules! zwrite {
    ($var:expr) => {
        match $var.try_write() {
            Ok(guard) => guard,
            Err(_) => $var.write().unwrap(),
        }
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
        if let Some(g) = $var.try_read() {
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
        if let Some(g) = $var.try_write() {
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

// This macro checks the boolean results of an operation and returns in case
// the result is false. Basically, it implements the ? operator for booleans
#[macro_export]
macro_rules! zcheck {
    ($op:expr) => {
        if !$op {
            return false;
        }
    };
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

// @TODO: re-design ZError and macros
// This macro is a shorthand for the creation of a ZError
#[macro_export]
macro_rules! zerror {
    ($kind:expr) => {
        Err(ZError::new($kind, file!(), line!(), None))
    };
    ($kind:expr, $source:expr) => {
        Err(ZError::new(
            $kind,
            file!(),
            line!(),
            Some(Box::new($source)),
        ))
    };
    ($kind:ident, $descr:expr, $source:expr) => {
        Err(ZError::new(
            ZErrorKind::$kind { descr: $descr },
            file!(),
            line!(),
            Some(Box::new($source)),
        ));
    };
}

#[macro_export]
macro_rules! zerror2 {
    ($kind:expr) => {
        ZError::new($kind, file!(), line!(), None)
    };
    ($kind:expr, $source:expr) => {
        ZError::new($kind, file!(), line!(), Some(Box::new($source)))
    };
    ($kind:ident, $descr:expr, $source:expr) => {
        ZError::new(
            ZErrorKind::$kind { descr: $descr },
            file!(),
            line!(),
            Some(Box::new($source)),
        );
    };
}

// This macro is a shorthand for the conversion of any Error into a ZError
#[macro_export]
macro_rules! to_zerror {
    ($kind:ident, $descr:expr) => {
        |e| zerror!($kind, $descr, e)
    };
}

// This macro is a shorthand for the conversion to ZInt
// This macro requires to previously import the following:
//   use std::convert::TryFrom;
#[macro_export]
macro_rules! to_zint {
    ($val:expr) => {
        ZInt::try_from($val).unwrap_or_else(|_| {
            panic!(
                "Can not encode {} as ZInt (max ZInt value: {})",
                $val,
                ZInt::MAX
            )
        })
    };
}

// This macro allows to spawn the right amount of threads in the
// async_std executor
#[macro_export]
macro_rules! zasync_executor_init {
    () => {
        use async_global_executor;

        // Zenoh requires at least 4 threads to run
        const ASYNC_STD_THREAD_COUNT_MIN: usize = 4;

        let count = async_global_executor::spawn_more_threads(ASYNC_STD_THREAD_COUNT_MIN)
            .await
            .unwrap();

        log::trace!(
            "Spawned {} additional threads in the async global executor",
            count
        );
    };
}
