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

use signal_hook::consts::signal::*;
use static_init::dynamic;

/// A global cleanup, that is guaranteed to be dropped at normal program exit and that will
/// execute all registered cleanup routines at this moment
#[dynamic(lazy, drop)]
pub(crate) static mut CLEANUP: Cleanup = Cleanup::new();

/// An RAII object that calls all registered routines upon destruction
pub(crate) struct Cleanup {
    cleanups: lockfree::queue::Queue<Option<Box<dyn FnOnce() + Send>>>,
}

impl Cleanup {
    fn new() -> Self {
        // todo: this is a workaround to make sure Cleanup will be executed even if process terminates via signal handlers
        // that execute std::terminate instead of exit
        for signal in [
            #[cfg(not(target_os = "windows"))]
            SIGHUP,
            SIGTERM,
            SIGINT,
            #[cfg(not(target_os = "windows"))]
            SIGQUIT,
        ] {
            unsafe {
                let _ = signal_hook::low_level::register(signal, || {
                    std::process::exit(0);
                });
            }
        }

        Self {
            cleanups: Default::default(),
        }
    }

    pub(crate) fn cleanup(&self) {
        while let Some(cleanup) = self.cleanups.pop() {
            if let Some(f) = cleanup {
                f();
            }
        }
    }

    pub(crate) fn register_cleanup(&self, cleanup_fn: Box<dyn FnOnce() + Send>) {
        self.cleanups.push(Some(cleanup_fn));
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.cleanup();
    }
}
