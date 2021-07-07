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

//! # The plugin infrastructure for Zenoh.
//!
//! To build a plugin, up to 2 types may be constructed :
//! * A [Plugin] type.
//! * [Plugin::start] should be non-blocking, and return a boxed instance of your stoppage type, which should implement [PluginStopper].

use clap::{Arg, ArgMatches};
use zenoh::net::runtime::Runtime;

pub mod prelude {
    pub use crate::{
        dynamic_loading::*, Plugin, PluginLaunch, PluginStopper,
    };
}

/// Your plugin's compatibility.
/// Currently, this should simply be the plugin crate's name.
/// This structure may evolve to include more detailed information.
#[derive(Clone, Debug, PartialEq)]
pub struct Compatibility {
    pub uid: &'static str,
}

#[derive(Clone)]
pub struct Incompatibility {
    pub own_compatibility: Compatibility,
    pub conflicting_with: Compatibility,
    pub details: Option<String>,
}

pub trait Plugin: PluginLaunch + Sized + 'static {
    /// Returns this plugin's [Compatibility].
    fn compatibility() -> Compatibility;

    /// As Zenoh instanciates plugins, it will append their [Compatibility] to an array.
    /// This array's current state will be shown to the next plugin.
    ///
    /// To signal that your plugin is incompatible with a previously instanciated plugin, return `Err`,
    /// Otherwise, return `Ok(Self::compatibility())`.
    ///
    /// By default, a plugin is non-reentrant to avoir reinstanciation if its dlib is accessible despite it already being statically linked.
    fn is_compatible_with(others: &[Compatibility]) -> Result<Compatibility, Incompatibility> {
        let own_compatibility = Self::compatibility();
        if others.iter().any(|c| c == &own_compatibility) {
            let conflicting_with = own_compatibility.clone();
            Err(Incompatibility {
                own_compatibility,
                conflicting_with,
                details: None,
            })
        } else {
            Ok(own_compatibility)
        }
    }

    /// Returns the arguments that are required for the plugin's construction
    fn get_expected_args() -> Vec<Arg<'static, 'static>>;

    /// Constructs an instance of the plugin, which will be launched with [PluginLaunch::start]
    fn init(args: &ArgMatches) -> Self;

    fn box_init(args: &ArgMatches) -> Box<dyn PluginLaunch> {
        Box::new(Self::init(args))
    }
}

/// Allows a [Plugin] instance to be started.
pub trait PluginLaunch {
    fn start(self, runtime: Runtime) -> Box<dyn PluginStopper>;
}

/// Allows a [Plugin] instance to be stopped.
/// Typically, you can achieve this using a one-shot channel or an [AtomicBool](std::sync::atomic::AtomicBool).
/// If you don't want a stopping mechanism, you can use `()` as your [PluginStopper].
pub trait PluginStopper {
    fn stop(self);
}

impl PluginStopper for () {
    fn stop(self) {}
}

pub mod dynamic_loading {
    use super::*;
    pub use no_mangle::*;

    pub type PluginVTableVersion = u16;
    /// This number should change any time the internal structure of [PluginVTable] changes
    pub const PLUGIN_VTABLE_VERSION: PluginVTableVersion = 0;

    /// For use with dynamically loaded plugins. Its size will not change accross versions, but its internal structure might.
    ///
    /// To ensure compatibility, its size and alignment must allow `size_of::<Result<PluginVTable, PluginVTableVersion>>() == 64` (one cache line).
    #[repr(C)]
    pub struct PluginVTable {
        init: fn(&ArgMatches) -> Box<dyn PluginLaunch>,
        is_compatible_with: fn(&[Compatibility]) -> Result<Compatibility, Incompatibility>,
        get_expected_args: fn() -> Vec<Arg<'static, 'static>>,
        __padding: [u8; 32],
    }

    impl PluginVTable {
        pub fn new<ConcretePlugin: Plugin + 'static>() -> Self {
            PluginVTable {
                is_compatible_with: ConcretePlugin::is_compatible_with,
                get_expected_args: ConcretePlugin::get_expected_args,
                init: ConcretePlugin::box_init,
                __padding: [0; 32],
            }
        }

        /// Ensures [PluginVTable]'s size stays the same between versions
        fn __size_check() {
            unsafe {
                std::mem::transmute::<_, [u8;64]>(
                    std::mem::MaybeUninit::<Result<Self, PluginVTableVersion>>::uninit(),
                )
            };
        }

        pub fn is_compatible_with(
            &self,
            others: &[Compatibility],
        ) -> Result<Compatibility, Incompatibility> {
            (self.is_compatible_with)(others)
        }

        pub fn get_expected_args(&self) -> Vec<Arg<'static, 'static>> {
            (self.get_expected_args)()
        }

        pub fn init(&self, args: &ArgMatches) -> Box<dyn PluginLaunch> {
            (self.init)(args)
        }
    }
    
    pub use no_mangle::*;
    #[cfg(feature = "no_mangle")]
    pub mod no_mangle {
        /// This macro will add a non-mangled `load_plugin` function to the library if feature `no_mangle` is enabled (which it is by default).
        #[macro_export]
        macro_rules! declare_plugin {
            ($ty: path) => {
                #[no_mangle]
                fn load_plugin(
                    version: PluginVTableVersion,
                ) -> Result<PluginVTable, PluginVTableVersion> {
                    if version == PLUGIN_VTABLE_VERSION {
                        Ok(PluginVTable::new::<ConcretePlugin>())
                    } else {
                        Err(PLUGIN_VTABLE_VERSION)
                    }
                }
            };
        }
    }
    #[cfg(not(feature = "no_mangle"))]
    pub mod no_mangle {
        #[macro_export]
        macro_rules! declare_plugin {
            ($ty: path) => {};
        }
    }
}
