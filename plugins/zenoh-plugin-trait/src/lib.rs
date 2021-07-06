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
use std::any::TypeId;

use clap::{Arg, ArgMatches};
use zenoh::net::runtime::Runtime;

pub trait Compatible: Any {
    fn is_compatible_with(others: &[TypeId]) -> bool;
}

pub trait PluginStopper {
    fn stop(self);
}

pub trait Plugin: Send + Sync + Compatible {
    type StopHandle: PluginStopper;
    fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>>;
    fn init<'a>(args: &'a ArgMatches<'_>) -> Self;
    fn start(self, runtime: Runtime) -> Self::StopHandle;
}

