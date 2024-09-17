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
/// WARN: The improper usage can break the application logic, impacting SHM-utilizing Sessions in other processes.
/// Cleanup unlinks SHM segments _created_ by current process from filesystem with the following consequences:
/// - Sessions that are not linked to this segment will fail to link it if they try. Such kind of errors are properly handled.
/// - Already linked processes will still have this shared memory mapped and safely accessible
/// - The actual memory will be reclaimed by the OS only after last process using it will close it or exit
///
/// In order to properly cleanup some SHM internals upon process exit, Zenoh installs exit handlers (see atexit() API).
/// The atexit handler is executed only on process exit(), the inconvenience is that terminating signal handlers
/// (like SIGINT) bypass it and terminate the process without cleanup. To eliminate this effect, Zenoh overrides
/// SIGHUP, SIGTERM, SIGINT and SIGQUIT handlers and calls exit() inside to make graceful shutdown. If user is going to
/// override these Zenoh's handlers, the workaround will break, and there are two ways to keep this workaround working:
/// - execute overridden Zenoh handlers in overriding handler code
/// - call force_cleanup_before_exit() anywhere at any time before terminating the process
#[zenoh_macros::unstable_doc]
pub fn force_cleanup_before_exit() {
    CLEANUP.read().cleanup();
}
