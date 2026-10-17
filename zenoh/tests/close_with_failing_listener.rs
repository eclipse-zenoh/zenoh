//
// Copyright (c) 2026 Oliver Harley
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Oliver Harley
//

//! A session whose listener can never bind must still close.
//!
//! `TaskController::spawn` carries a documented obligation, restated on
//! `Runtime::spawn` itself:
//!
//! > Spawns a task within runtime. **Upon close runtime will block until this
//! > task completes**
//!
//! `spawn_add_listener` used it for `add_listener_retry`, which is
//! `loop { if add_listener().await.is_ok() { break } sleep(period).await }` —
//! unbounded, and holding no cancellation token. So a listener that can never
//! bind means close blocks forever.
//!
//! The configured listen timeout does not save it. `bind_listeners` wraps
//! `bind_listeners_impl` in `tokio::time::timeout`, but on this branch
//! (`exit_on_failure == false`) that function *spawns* and returns immediately —
//! so the timeout bounds the spawning, not the spawned work.
//!
//! Reaching the spawned path needs both conditions together, which is why the
//! existing `connection_retry.rs` tests miss it: they leave `exit_on_failure` at
//! its default and so take the inline branch, where the retry really is awaited
//! and really is bounded.

use std::time::{Duration, Instant};

use zenoh::Config;
use zenoh_core::Wait;

/// A listener that can never bind must not stop the session from closing.
///
/// `tcp/8.8.8.8:8` is chosen so the failure is local and immediate: binding an
/// address that belongs to no local interface fails in the kernel with
/// `EADDRNOTAVAIL` without a single packet leaving the machine. No connection is
/// attempted, so this neither touches the network nor prompts a firewall dialog.
#[test]
fn close_completes_when_listener_never_binds() {
    let mut config = Config::default();
    config
        .insert_json5("listen/endpoints", r#"["tcp/8.8.8.8:8"]"#)
        .unwrap();
    // Non-zero timeout AND exit_on_failure=false is the exact pair that reaches
    // `spawn_add_listener`. Either one alone takes a bounded path.
    config.insert_json5("listen/timeout_ms", "1000").unwrap();
    config
        .insert_json5("listen/exit_on_failure", "false")
        .unwrap();

    // Opening is expected to succeed: the listener failure is backgrounded, which
    // is the documented point of `exit_on_failure=false`.
    let session = zenoh::open(config).wait().unwrap();

    let start = Instant::now();
    session.close().wait().unwrap();
    let elapsed = start.elapsed();

    // Generous on purpose. The unfixed behaviour is not "a bit slow" but "never
    // returns", so any bound at all discriminates, and a loose one keeps the test
    // from going flaky on a loaded CI machine. If this ever fails it will fail by
    // a mile, not by a margin.
    assert!(
        elapsed < Duration::from_secs(10),
        "close() took {elapsed:?} with a permanently-failing listener; the \
         background add_listener_retry loop is not observing the runtime's \
         cancellation token"
    );
}
