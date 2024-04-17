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
use tracing_subscriber::EnvFilter;

/// This is an utility function to enable the tracing formatting subscriber from
/// the `RUST_LOG` environment variable.
///
/// # Safety
/// Calling this function initializes a `lazy_static` in the `tracing` crate
/// such static is not deallocated prior to process existing, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: https://github.com/tokio-rs/tracing/issues/2069
pub fn init_log_from_env() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("z=info"));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// This is an utility function to enables the default tracing subscriber with INFO level
pub fn init_log() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
