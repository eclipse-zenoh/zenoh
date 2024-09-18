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

use std::thread::JoinHandle;

use static_init::dynamic;
use zenoh_core::zerror;

/// A global cleanup, that is guaranteed to be dropped at normal program exit and that will
/// execute all registered cleanup routines at this moment
#[dynamic(lazy, drop)]
pub(crate) static mut CLEANUP: Cleanup = Cleanup::new();

/// An RAII object that calls all registered routines upon destruction
pub(crate) struct Cleanup {
    cleanups: lockfree::queue::Queue<Option<Box<dyn FnOnce() + Send>>>,
    handle: Option<JoinHandle<()>>,
}

impl Cleanup {
    fn new() -> Self {
        // todo: this is a workaround to make sure Cleanup will be executed even if process terminates via signals
        let handle = match ctrlc2::set_handler(|| {
            tokio::task::spawn_blocking(|| std::process::exit(0));
            true
        }) {
            Ok(h) => Some(h),
            Err(e) => {
                match e {
                    ctrlc2::Error::NoSuchSignal(signal_type) => {
                        zerror!(
                            "Error registering cleanup handler for signal {:?}: no such signal!",
                            signal_type
                        );
                    }
                    ctrlc2::Error::MultipleHandlers => {
                        zerror!("Error registering cleanup handler: already registered!");
                    }
                    ctrlc2::Error::System(error) => {
                        zerror!("Error registering cleanup handler: system error: {error}");
                    }
                }
                None
            }
        };

        Self {
            cleanups: Default::default(),
            handle,
        }
    }

    pub(crate) fn register_cleanup(&self, cleanup_fn: Box<dyn FnOnce() + Send>) {
        self.cleanups.push(Some(cleanup_fn));
    }

    pub(crate) fn cleanup(&self) {
        while let Some(cleanup) = self.cleanups.pop() {
            if let Some(f) = cleanup {
                f();
            }
        }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.handle.take();
        self.cleanup();
    }
}
