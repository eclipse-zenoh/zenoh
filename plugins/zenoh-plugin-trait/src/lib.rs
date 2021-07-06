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
use std::any::Any;

use clap::{Arg, ArgMatches};
use zenoh::net::runtime::Runtime;

pub struct Compatibility {
    pub uid: u64
}

pub trait Compatible: Any {
    fn is_compatible_with(others: &[Compatibility]) -> bool;
}

pub trait ArgConstructible {
    fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>>;
    fn init<'a>(args: &'a ArgMatches<'_>) -> Self;
}

pub trait PluginStopper {
    fn stop(self);
}

pub trait Plugin: Send + Sync {
    fn start(self, runtime: Runtime) -> Box<dyn PluginStopper>;
}

#[repr(C)]
pub struct PluginVTable {
    is_compatible_with: &'static dyn Fn(&[Compatibility]) -> bool,
    get_expected_args: &'static dyn Fn()->Vec<Arg<'static, 'static>>,
    init: &'static dyn Fn(&ArgMatches) -> Box<dyn Plugin>,
}

impl PluginVTable {
    pub fn is_compatible_with(&self, others: &[Compatibility]) -> bool {
        (self.is_compatible_with)(others)
    }

    pub fn get_expected_args(&self) -> Vec<Arg<'static, 'static>> {
        (self.get_expected_args)()
    }

    pub fn init(&self, args: &ArgMatches) -> Box<dyn Plugin> {
        (self.init)(args)
    }
}


#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    use super::*;

    pub fn load_plugin<ConcretePlugin: Plugin + ArgConstructible + Compatible>(vtable: &mut PluginVTable) {
        vtable.is_compatible_with = &ConcretePlugin::is_compatible_with;
        vtable.get_expected_args = &ConcretePlugin::get_expected_args;
        vtable.init = &|args|{Box::new(ConcretePlugin::init(args))};
    }
}