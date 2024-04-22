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

use tracing_subscriber::EnvFilter;

/// This is an utility function to enable the tracing formatting subscriber from
/// the `RUST_LOG` environment variable. If `RUST_LOG` is not set, then logging is not enabled.
///
/// # Safety
/// Calling this function initializes a `lazy_static` in the `tracing` crate
/// such static is not deallocated prior to process existing, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: https://github.com/tokio-rs/tracing/issues/2069
pub fn try_init_log_from_env() {
    if let Ok(env_filter) = EnvFilter::try_from_default_env() {
        init_env_filter(env_filter);
    }
}

/// This is an utility function to enable the tracing formatting subscriber from
/// the environment variable. If `RUST_LOG` is not set, then fallback directives are used.
///
/// # Safety
/// Calling this function initializes a `lazy_static` in the `tracing` crate
/// such static is not deallocated prior to process existing, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: https://github.com/tokio-rs/tracing/issues/2069
pub fn init_log_from_env_or<S>(fallback: S)
where
    S: AsRef<str>,
{
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback));
    init_env_filter(env_filter);
}

fn init_env_filter(env_filter: EnvFilter) {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(feature = "test")]
// Used to verify memory leaks for valgrind CI.
// `EnvFilter` internally uses a static reference that is not cleaned up yielding to false positive in valgrind.
// This function enables logging without calling `EnvFilter` for env configuration.
pub fn init_log_test() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
