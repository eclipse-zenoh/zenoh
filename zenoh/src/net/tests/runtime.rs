//
// Copyright (c) 2026 ZettaScale Technology
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
#[cfg(feature = "test")]
mod tests {
    use crate::{
        api::config::Config,
        net::runtime::{Runtime, RuntimeBuilder},
    };

    // Helper to create a runtime that does not bind to any network endpoints.
    async fn create_runtime() -> Runtime {
        let config = Config::default();
        RuntimeBuilder::new(config).build().await.unwrap()
    }

    #[tokio::test]
    async fn test_state_weak_alive_with_runtime() {
        let runtime = create_runtime().await;
        let weak = runtime.state_weak();
        assert!(weak.upgrade().is_some());
    }

    #[tokio::test]
    async fn test_state_weak_dead_after_drop() {
        let runtime = create_runtime().await;
        let weak = runtime.state_weak();
        drop(runtime); // Releases the last strong reference held by the Runtime object.
        assert!(weak.upgrade().is_none());
    }

    #[cfg(feature = "internal")]
    #[cfg(feature = "unstable")]
    #[tokio::test]
    async fn test_state_weak_still_alive_after_close() {
        let runtime = create_runtime().await;
        let weak = runtime.state_weak();
        runtime.close().await.unwrap(); // Cancels all internal tasks that hold references.
                                        // The Runtime object still holds the strong reference, so the weak is still valid.
        assert!(weak.upgrade().is_some());
    }

    #[cfg(feature = "internal")]
    #[cfg(feature = "unstable")]
    #[tokio::test]
    async fn test_state_weak_dead_after_close_and_drop() {
        let runtime = create_runtime().await;
        let weak = runtime.state_weak();
        runtime.close().await.unwrap(); // Cancels all internal tasks that hold references.
        drop(runtime); // Releases the last strong reference held by the Runtime object.
        assert!(weak.upgrade().is_none());
    }
}
