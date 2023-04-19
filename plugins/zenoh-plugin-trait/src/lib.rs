//
// Copyright (c) 2023 ZettaScale Technology
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

//! # The plugin infrastructure for Zenoh.
//!
//! To build a plugin, implement [`Plugin`].
//!
//! If building a plugin for [`zenohd`](https://crates.io/crates/zenoh), you should use the types exported in [`zenoh::plugins`](https://docs.rs/zenoh/latest/zenoh/plugins) to fill [`Plugin`]'s associated types.  
//! To check your plugin typing for `zenohd`, have your plugin implement [`zenoh::plugins::ZenohPlugin`](https://docs.rs/zenoh/latest/zenoh/plugins/struct.ZenohPlugin)
pub mod loading;
pub mod vtable;

use zenoh_result::ZResult;

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Compatibility {
    major: u64,
    minor: u64,
    patch: u64,
    stable: bool,
    commit: &'static str,
}
const RELEASE_AND_COMMIT: (&str, &str) = zenoh_macros::rustc_version_release!();
impl Compatibility {
    pub fn new() -> ZResult<Self> {
        let (release, commit) = RELEASE_AND_COMMIT;
        let (release, stable) = if let Some(p) = release.chars().position(|c| c == '-') {
            (&release[..p], false)
        } else {
            (release, true)
        };
        let mut split = release.split('.').map(|s| s.trim());
        Ok(Compatibility {
            major: split.next().unwrap().parse().unwrap(),
            minor: split.next().unwrap().parse().unwrap(),
            patch: split.next().unwrap().parse().unwrap(),
            stable,
            commit,
        })
    }
    pub fn are_compatible(a: &Self, b: &Self) -> bool {
        if a.stable && b.stable {
            a.major == b.major && a.minor == b.minor && a.patch == b.patch
        } else {
            a == b
        }
    }
}

pub mod prelude {
    pub use crate::{loading::*, vtable::*, Plugin};
}

pub trait Plugin: Sized + 'static {
    type StartArgs;
    type RunningPlugin;
    /// Your plugins' default name when statically linked.
    const STATIC_NAME: &'static str;
    /// You probabky don't need to override this function.
    ///
    /// Returns some build information on your plugin, allowing the host to detect potential ABI changes that would break it.
    fn compatibility() -> ZResult<Compatibility> {
        Compatibility::new()
    }
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::RunningPlugin>;
}
