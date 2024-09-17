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

use crate::cleanup::CLEANUP;

/// Make forced cleanup
/// NOTE: this is a part of a temporary on-exit-cleanup workaround and it will be very likely removed in the future.
/// In order to properly cleanup some SHM internals upon process exit, Zenoh installs exit handlers (see atexit() API).
/// The bad thing is that atexit handler is executed only on process exit(), the terminating signal handlers (like SIGINT)
/// bypass it and terminate the process without cleanup. To eliminate this effect, Zenoh overrides SIGHUP, SIGTERM, SIGINT
/// and SIGQUIT handlers and calls exit() inside to make graceful shutdown. If user is going to override these Zenoh's handlers,
/// the workaround will break, and there are two ways to keep this workaround working:
/// - execute overridden Zenoh handlers in overriding handler code
/// - call forced_cleanup() anywhere at any time before terminating the process
#[zenoh_macros::unstable_doc]
pub fn forced_cleanup() {
    CLEANUP.read().cleanup();
}
