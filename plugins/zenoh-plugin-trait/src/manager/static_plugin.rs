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
use crate::*;
use std::{borrow::Cow, marker::PhantomData};
use zenoh_result::ZResult;

pub struct StaticPlugin<StartArgs, Instance: PluginInstance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    instance: Option<Instance>,
    phantom: PhantomData<P>,
}

impl<StartArgs, Instance: PluginInstance, P> StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    pub fn new() -> Self {
        Self {
            instance: None,
            phantom: PhantomData,
        }
    }
}

impl<StartArgs, Instance: PluginInstance, P> PluginInfo for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn name(&self) -> &str {
        P::DEFAULT_NAME
    }
    fn status(&self) -> PluginStatus {
        PluginStatus {
            version: Some(Cow::Borrowed(P::PLUGIN_VERSION)),
            path: None,
            state: self
                .instance
                .as_ref()
                .map_or(PluginState::Loaded, |_| PluginState::Started),
            report: if let Some(instance) = &self.instance {
                instance.report()
            } else {
                PluginReport::default()
            },
        }
    }
}

impl<StartArgs, Instance: PluginInstance, P> DeclaredPlugin<StartArgs, Instance>
    for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        Ok(self)
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
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_none() {
            log::debug!("Plugin `{}` started", self.name());
            self.instance = Some(P::start(self.name(), args)?);
        } else {
            log::warn!("Plugin `{}` already started", self.name());
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
    fn stop(&mut self) {
        log::debug!("Plugin `{}` stopped", self.name());
        self.instance = None;
    }
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}
