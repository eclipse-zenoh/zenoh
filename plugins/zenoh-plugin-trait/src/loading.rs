use crate::vtable::*;
use crate::*;
use libloading::Library;
use std::collections::HashMap;
use std::path::PathBuf;
use zenoh_util::core::Result as ZResult;
use zenoh_util::{bail, LibLoader};

pub struct PluginsManager<StartArgs, RunningPlugin> {
    loader: LibLoader,
    plugin_starters: Vec<Box<dyn PluginStarter<StartArgs, RunningPlugin> + Send + Sync>>,
    running_plugins: HashMap<String, RunningPlugin>,
}
impl<StartArgs: 'static, RunningPlugin: 'static> PluginsManager<StartArgs, RunningPlugin> {
    /// Constructs a new plugin manager.
    pub fn new(loader: LibLoader) -> Self {
        PluginsManager {
            loader,
            plugin_starters: Vec::new(),
            running_plugins: HashMap::new(),
        }
    }

    /// Adds a statically linked plugin to the manager.
    pub fn add_static<
        P: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin> + Send + Sync,
    >(
        mut self,
    ) -> Self {
        let plugin_starter: StaticPlugin<P> = StaticPlugin::new();
        self.plugin_starters.push(Box::new(plugin_starter));
        self
    }

    /// Starts `plugin`.
    ///
    /// `Ok(true)` => plugin was successfully started  
    /// `Ok(false)` => plugin was running already, nothing happened  
    /// `Err(e)` => starting the plugin failed due to `e`
    pub fn start(&mut self, plugin: &str, args: &StartArgs) -> ZResult<bool> {
        if self.running_plugins.contains_key(plugin) {
            return Ok(false);
        }
        match self.plugin_starters.iter().find(|p| p.name() == plugin) {
            Some(s) => s.start(args)?,
            None => bail!("Plugin starter for `{}` not found", plugin),
        };
        Ok(true)
    }

    /// Lazily starts all plugins.
    ///
    /// `Ok(Ok(name))` => plugin `name` was successfully started  
    /// `Ok(Err(name))` => plugin `name` wasn't started because it was already running  
    /// `Err(e)` => Error `e` occured when trying to start plugin `name`
    pub fn start_all<'l>(
        &'l mut self,
        args: &'l StartArgs,
    ) -> impl Iterator<Item = (&str, &str, ZResult<bool>)> + 'l {
        let PluginsManager {
            plugin_starters: plugins_starters,
            running_plugins,
            ..
        } = self;
        plugins_starters.iter().map(move |p| {
            (
                p.name(),
                p.path(),
                match running_plugins.entry(p.name().into()) {
                    std::collections::hash_map::Entry::Occupied(_) => Ok(false),
                    std::collections::hash_map::Entry::Vacant(e) => match p.start(args) {
                        Ok(p) => {
                            e.insert(p);
                            Ok(true)
                        }
                        Err(e) => Err(e),
                    },
                },
            )
        })
    }

    /// Stops `plugin`, returning `true` if it was indeed running.
    pub fn stop(&mut self, plugin: &str) -> bool {
        let result = self.running_plugins.remove(plugin).is_some();
        self.plugin_starters
            .retain(|p| p.name() == plugin || !p.deleteable());
        result
    }

    pub fn available_plugins(&self) -> impl Iterator<Item = &str> {
        self.plugin_starters.iter().map(|p| p.name())
    }
    pub fn running_plugins_info(&self) -> Vec<(&str, &str)> {
        let mut result = Vec::with_capacity(self.running_plugins.len());
        for p in self.plugin_starters.iter() {
            let name = p.name();
            if self.running_plugins.contains_key(name) && !result.iter().any(|(p, _)| p == &name) {
                result.push((name, p.path()))
            }
        }
        result
    }
    pub fn running_plugins(&self) -> impl Iterator<Item = (&str, &RunningPlugin)> {
        self.running_plugins.iter().map(|(s, p)| (s.as_str(), p))
    }
    pub fn plugin(&self, name: &str) -> Option<&RunningPlugin> {
        self.running_plugins.get(name)
    }

    pub fn load_plugin_by_name(&mut self, name: String) -> ZResult<String> {
        let (lib, p) = unsafe { self.loader.search_and_load(&format!("zplugin_{}", &name))? };
        let plugin = DynamicPlugin::new(name.clone(), lib, p).unwrap_or_else(|e| panic!("Wrong PluginVTable version, your {} doesn't appear to be compatible with this version of Zenoh (vtable versions: plugin v{}, zenoh v{})", name, e.map_or_else(|| "UNKNWON".to_string(), |e| e.to_string()), PLUGIN_VTABLE_VERSION));
        let path = plugin.path().into();
        self.plugin_starters.push(Box::new(plugin));
        Ok(path)
    }
    pub fn load_plugin_by_paths<P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: String,
        paths: &[P],
    ) -> ZResult<String> {
        for path in paths {
            let path = path.as_ref();
            match unsafe { LibLoader::load_file(path) } {
                Ok((lib, p)) => {
                    let plugin = DynamicPlugin::new(name.clone(), lib, p).unwrap_or_else(|e| panic!("Wrong PluginVTable version, your {} doesn't appear to be compatible with this version of Zenoh (vtable versions: plugin v{}, zenoh v{})", name, e.map_or_else(|| "UNKNWON".to_string(), |e| e.to_string()), PLUGIN_VTABLE_VERSION));
                    let path = plugin.path().into();
                    self.plugin_starters.push(Box::new(plugin));
                    return Ok(path);
                }
                Err(e) => log::warn!("Plugin '{}' load fail at {}: {}", &name, path, e),
            }
        }
        bail!("Plugin '{}' not found in {:?}", name, &paths)
    }
}
trait PluginStarter<StartArgs, RunningPlugin> {
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin>;
    fn deleteable(&self) -> bool;
}
struct StaticPlugin<P> {
    inner: std::marker::PhantomData<P>,
}
impl<P> StaticPlugin<P> {
    fn new() -> Self {
        StaticPlugin {
            inner: std::marker::PhantomData,
        }
    }
}
impl<StartArgs, RunningPlugin, P> PluginStarter<StartArgs, RunningPlugin> for StaticPlugin<P>
where
    P: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>,
{
    fn name(&self) -> &str {
        P::STATIC_NAME
    }
    fn path(&self) -> &str {
        "<statically_linked>"
    }
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin> {
        P::start(P::STATIC_NAME, args)
    }
    fn deleteable(&self) -> bool {
        false
    }
}
impl<StartArgs, RunningPlugin> PluginStarter<StartArgs, RunningPlugin>
    for DynamicPlugin<StartArgs, RunningPlugin>
{
    fn name(&self) -> &str {
        &self.name
    }
    fn path(&self) -> &str {
        self.path.to_str().unwrap()
    }
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin> {
        self.vtable.start(self.name(), args)
    }
    fn deleteable(&self) -> bool {
        true
    }
}

pub struct DynamicPlugin<StartArgs, RunningPlugin> {
    _lib: Library,
    vtable: PluginVTable<StartArgs, RunningPlugin>,
    pub name: String,
    pub path: PathBuf,
}

impl<StartArgs, RunningPlugin> DynamicPlugin<StartArgs, RunningPlugin> {
    fn new(name: String, lib: Library, path: PathBuf) -> Result<Self, Option<PluginVTableVersion>> {
        let load_plugin = unsafe {
            lib.get::<fn(PluginVTableVersion) -> LoadPluginResult<StartArgs, RunningPlugin>>(
                b"load_plugin",
            )
            .map_err(|_| None)?
        };
        match load_plugin(PLUGIN_VTABLE_VERSION) {
            Ok(vtable) => Ok(DynamicPlugin {
                _lib: lib,
                vtable,
                name,
                path,
            }),
            Err(plugin_version) => Err(Some(plugin_version)),
        }
    }
}
