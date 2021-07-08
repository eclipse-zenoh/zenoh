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
//! * A [`Plugin`] type.
//! * [`PluginLaunch::start`] should be non-blocking, and return a boxed instance of your stoppage type, which should implement [`PluginStopper`].

use clap::{Arg, ArgMatches};
use std::error::Error;

pub mod prelude {
    pub use crate::{dynamic_loading::*, Plugin, PluginStopper};
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

impl std::fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} prevented {} from initing",
            self.conflicting_with.uid, self.own_compatibility.uid
        )?;
        if let Some(details) = &self.details {
            write!(f, ": {}", details)?;
        }
        write!(f, ".")
    }
}

/// Zenoh plugins must implement [`Plugin<Runtime=zenoh::net::runtime::Runtime>`](Plugin)
pub trait Plugin: Sized + 'static {
    /// Returns this plugin's [`Compatibility`].
    fn compatibility() -> Compatibility;
    type Runtime;

    /// As Zenoh instanciates plugins, it will append their [`Compatibility`] to an array.
    /// This array's current state will be shown to the next plugin.
    ///
    /// To signal that your plugin is incompatible with a previously instanciated plugin, return `Err`,
    /// Otherwise, return `Ok(Self::compatibility())`.
    ///
    /// By default, a plugin is non-reentrant to avoid reinstanciation if its dlib is accessible despite it already being statically linked.
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

    /// Constructs an instance of the plugin, which will be launched with [`PluginLaunch::start`]
    fn init(args: &ArgMatches) -> Result<Self, Box<dyn Error>>;

    fn box_init(args: &ArgMatches) -> Result<Box<dyn std::any::Any>, Box<dyn Error>> {
        match Self::init(args) {
            Ok(v) => Ok(Box::new(v)),
            Err(e) => Err(e),
        }
    }

    fn start(&mut self, runtime: Self::Runtime) -> Box<dyn PluginStopper>;

    /// # Safety
    /// ensuring `s` is the right type before calling is primordial
    unsafe fn do_start(s: *mut (), runtime: Self::Runtime) -> Box<dyn PluginStopper> {
        unsafe { (&mut *(s.cast::<Self>())).start(runtime) }
    }
}

/// Allows a [`Plugin`] instance to be stopped.
/// Typically, you can achieve this using a one-shot channel or an [`AtomicBool`](std::sync::atomic::AtomicBool).
/// If you don't want a stopping mechanism, you can use `()` as your [`PluginStopper`].
pub trait PluginStopper: Send + Sync {
    fn stop(&self);
}

impl PluginStopper for () {
    fn stop(&self) {}
}

pub mod dynamic_loading {
    use super::*;
    pub use no_mangle::*;

    type InitFn = fn(&ArgMatches) -> Result<Box<dyn std::any::Any>, Box<dyn Error>>;
    pub type PluginVTableVersion = u16;
    type LoadPluginResultInner = Result<PluginVTableInner<()>, PluginVTableVersion>;
    pub type LoadPluginResult<T> = Result<PluginVTable<T>, PluginVTableVersion>;

    /// This number should change any time the internal structure of [`PluginVTable`] changes
    pub const PLUGIN_VTABLE_VERSION: PluginVTableVersion = 0;

    #[repr(C)]
    struct PluginVTableInner<Runtime> {
        init: InitFn,
        is_compatible_with: fn(&[Compatibility]) -> Result<Compatibility, Incompatibility>,
        get_expected_args: fn() -> Vec<Arg<'static, 'static>>,
        start: unsafe fn(*mut (), Runtime) -> Box<dyn PluginStopper + 'static>,
    }

    /// Automagical padding such that [PluginVTable::init]'s result is the size of a cache line
    #[repr(C)]
    struct PluginVTablePadding {
        __padding: [u8; PluginVTablePadding::padding_length()],
    }
    impl PluginVTablePadding {
        const fn padding_length() -> usize {
            64 - std::mem::size_of::<LoadPluginResultInner>()
        }
        fn new() -> Self {
            PluginVTablePadding {
                __padding: [0; Self::padding_length()],
            }
        }
    }

    /// For use with dynamically loaded plugins. Its size will not change accross versions, but its internal structure might.
    ///
    /// To ensure compatibility, its size and alignment must allow `size_of::<Result<PluginVTable, PluginVTableVersion>>() == 64` (one cache line).
    #[repr(C)]
    pub struct PluginVTable<Runtime> {
        inner: PluginVTableInner<Runtime>,
        padding: PluginVTablePadding,
    }

    impl<Runtime> PluginVTable<Runtime> {
        pub fn new<ConcretePlugin: Plugin<Runtime = Runtime>>() -> Self {
            PluginVTable {
                inner: PluginVTableInner {
                    is_compatible_with: ConcretePlugin::is_compatible_with,
                    get_expected_args: ConcretePlugin::get_expected_args,
                    init: ConcretePlugin::box_init,
                    start: ConcretePlugin::do_start,
                },
                padding: PluginVTablePadding::new(),
            }
        }

        /// Ensures [PluginVTable]'s size stays the same between versions
        fn __size_check() {
            unsafe {
                std::mem::transmute::<_, [u8; 64]>(std::mem::MaybeUninit::<
                    Result<Self, PluginVTableVersion>,
                >::uninit())
            };
        }

        pub fn is_compatible_with(
            &self,
            others: &[Compatibility],
        ) -> Result<Compatibility, Incompatibility> {
            (self.inner.is_compatible_with)(others)
        }

        pub fn get_expected_args(&self) -> Vec<Arg<'static, 'static>> {
            (self.inner.get_expected_args)()
        }

        pub fn init(&self, args: &ArgMatches) -> Result<Box<dyn std::any::Any>, Box<dyn Error>> {
            (self.inner.init)(args)
        }

        pub unsafe fn start(&self, s: *mut (), runtime: Runtime) -> Box<dyn PluginStopper> {
            unsafe { (self.inner.start)(s, runtime) }
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
                ) -> LoadPluginResult<<$ty as Plugin>::Runtime> {
                    if version == PLUGIN_VTABLE_VERSION {
                        Ok(PluginVTable::new::<$ty>())
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
