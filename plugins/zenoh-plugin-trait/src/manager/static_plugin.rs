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
use std::marker::PhantomData;

use zenoh_result::ZResult;

use crate::*;

pub struct StaticPlugin<StartArgs, Instance: PluginInstance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    instance: Option<Instance>,
    required: bool,
    phantom: PhantomData<P>,
    id: String,
}

impl<StartArgs, Instance: PluginInstance, P> StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    pub fn new(id: String, required: bool) -> Self {
        Self {
            instance: None,
            required,
            phantom: PhantomData,
            id,
        }
    }
}

impl<StartArgs, Instance: PluginInstance, P> PluginStatus for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn name(&self) -> &str {
        P::DEFAULT_NAME
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn version(&self) -> Option<&str> {
        Some(P::PLUGIN_VERSION)
    }
    fn long_version(&self) -> Option<&str> {
        Some(P::PLUGIN_LONG_VERSION)
    }
    fn path(&self) -> &str {
        "__static_lib__"
    }
    fn state(&self) -> PluginState {
        self.instance
            .as_ref()
            .map_or(PluginState::Loaded, |_| PluginState::Started)
    }
    fn report(&self) -> PluginReport {
        if let Some(instance) = &self.instance {
            instance.report()
        } else {
            PluginReport::default()
        }
    }
}

impl<StartArgs, Instance: PluginInstance, P> DeclaredPlugin<StartArgs, Instance>
    for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn load(&mut self) -> ZResult<Option<&mut dyn LoadedPlugin<StartArgs, Instance>>> {
        Ok(Some(self))
    }
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        Some(self)
    }
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        Some(self)
    }
}

impl<StartArgs, Instance: PluginInstance, P> LoadedPlugin<StartArgs, Instance>
    for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn required(&self) -> bool {
        self.required
    }
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_none() {
            tracing::debug!("Plugin `{}` started", self.id());
            self.instance = Some(P::start(self.id(), args)?);
        } else {
            tracing::warn!("Plugin `{}` already started", self.id());
        }
        Ok(self)
    }
    fn started(&self) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
    fn started_mut(&mut self) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl<StartArgs, Instance: PluginInstance, P> StartedPlugin<StartArgs, Instance>
    for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn stop(&mut self) {
        tracing::debug!("Plugin `{}` stopped", self.name());
        self.instance = None;
    }
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}
