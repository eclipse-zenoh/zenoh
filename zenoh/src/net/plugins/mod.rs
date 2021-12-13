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
use super::runtime::Runtime;
use crate::prelude::{KeyExpr, SampleKind, Selector, Value};
use crate::Result as ZResult;
use zenoh_util::zconfigurable;
zconfigurable! {
    pub static ref PLUGIN_PREFIX: String = "zplugin_".to_string();
}

pub use zenoh_plugin_trait::RunningPluginTrait;
pub type StartArgs = Runtime;
pub type GetterIn<'a> = &'a Selector<'a>;
pub type GetterOut<'a> = ZResult<serde_json::Value>;
pub type SetterIn<'a> = (&'a KeyExpr<'a>, &'a SampleKind, &'a Value);
pub type SetterOut<'a> = ZResult<()>;
pub type RuntimePlugin = Box<
    dyn for<'a> RunningPluginTrait<
            'a,
            GetterIn = GetterIn<'a>,
            GetterOut = GetterOut<'a>,
            SetterIn = SetterIn<'a>,
            SetterOut = SetterOut<'a>,
        > + 'static,
>;
pub type PluginsManager = zenoh_plugin_trait::loading::PluginsManager<StartArgs, RuntimePlugin>;
