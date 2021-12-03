use crate::vtable::*;
use crate::*;
use libloading::Library;
use std::path::PathBuf;
use zenoh_util::core::Result as ZResult;
use zenoh_util::{bail, LibLoader};

type Compats = (Vec<PluginId>, Vec<Option<Incompatibility>>);

pub struct StaticPlugins<A, B, C> {
    a: std::marker::PhantomData<A>,
    b: std::marker::PhantomData<B>,
    c: std::marker::PhantomData<C>,
}
impl<C> StaticPlugins<(), (), C> {
    #[inline(always)]
    pub fn builder() -> Self {
        Self::new()
    }
}
impl<A, B, C> StaticPlugins<A, B, C> {
    #[inline(always)]
    fn new() -> Self {
        StaticPlugins {
            a: Default::default(),
            b: Default::default(),
            c: Default::default(),
        }
    }
    #[inline(always)]
    pub fn add_static<E>(self) -> StaticPlugins<E, Self, C> {
        StaticPlugins::new()
    }

    pub fn into_dynamic(self, loader: LibLoader) -> DynamicLoader<Self>
    where
        Self: MultipleStaticPlugins,
    {
        DynamicLoader {
            _static: self,
            loader,
            dynamic_plugins: vec![],
        }
    }
}

impl<A, B, C> Default for StaticPlugins<A, B, C> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait MergeRequirements {
    fn zero() -> Self;
    fn merge(self, other: Self) -> Self;
}

impl<T> MergeRequirements for Vec<T> {
    fn zero() -> Self {
        Default::default()
    }
    fn merge(mut self, other: Self) -> Self {
        self.extend(other);
        self
    }
}

type StartResult = (Vec<(String, RunningPlugin)>, Vec<Box<dyn Error>>);

pub trait MultipleStaticPlugins: Sized {
    type StartArgs;
    fn merge_requirements_and_conflicts() -> Compats;

    fn start(
        it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        args: &Self::StartArgs,
    ) -> StartResult;

    fn __len(cur: usize) -> usize;
    fn len() -> usize {
        Self::__len(0)
    }
    fn build(self) -> (StaticLauncher<Self>, Vec<PluginId>) {
        let (ids, should_run) = Self::merge_requirements_and_conflicts();
        (StaticLauncher::new(self, should_run, &ids), ids)
    }
}
impl<StartArgs> MultipleStaticPlugins for StaticPlugins<(), (), StartArgs> {
    type StartArgs = StartArgs;
    #[inline(always)]
    fn merge_requirements_and_conflicts() -> Compats {
        (vec![], vec![])
    }

    fn __len(cur: usize) -> usize {
        cur
    }

    fn start(
        _it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        _args: &StartArgs,
    ) -> StartResult {
        (vec![], vec![])
    }
}
impl<
        StartArgs,
        A: Plugin<StartArgs = StartArgs>,
        B: MultipleStaticPlugins<StartArgs = StartArgs>,
    > MultipleStaticPlugins for StaticPlugins<A, B, StartArgs>
{
    type StartArgs = StartArgs;
    #[inline(always)]
    fn merge_requirements_and_conflicts() -> Compats {
        let (mut compats, mut should_init) = B::merge_requirements_and_conflicts();
        match A::is_compatible_with(&compats) {
            Ok(c) => {
                compats.push(c);
                should_init.push(None);
            }
            Err(i) => {
                should_init.push(Some(i));
            }
        }
        (compats, should_init)
    }

    fn __len(cur: usize) -> usize {
        B::__len(cur + 1)
    }

    #[inline(always)]
    fn start(
        it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        args: &StartArgs,
    ) -> StartResult {
        let (mut ok, mut err) = B::start(it, args);
        match it.next().unwrap() {
            None => match A::start(A::STATIC_NAME, args) {
                Ok(s) => ok.push((A::STATIC_NAME.into(), s)),
                Err(e) => err.push(e),
            },
            Some(i) => err.push(Box::new(i)),
        };
        (ok, err)
    }
}

pub struct StaticLauncher<StaticPlugins> {
    _static_plugins: StaticPlugins,
    should_run: Vec<Option<Incompatibility>>,
    ids: Vec<String>,
}

type HandlesAndErrors<StartArgs> = (PluginsHandles<StartArgs>, Vec<Box<dyn Error>>);

impl<StaticPlugins> StaticLauncher<StaticPlugins> {
    fn new(
        static_plugins: StaticPlugins,
        should_run: Vec<Option<Incompatibility>>,
        ids: &[PluginId],
    ) -> Self {
        StaticLauncher {
            _static_plugins: static_plugins,
            ids: ids
                .iter()
                .zip(&should_run)
                .filter_map(|(id, incompat)| {
                    if incompat.is_none() {
                        Some(id.uid.to_owned())
                    } else {
                        None
                    }
                })
                .collect(),
            should_run,
        }
    }

    pub fn incompatibilities(&self) -> impl Iterator<Item = &Incompatibility> {
        self.should_run.iter().filter_map(Option::as_ref)
    }

    pub fn start<StartArgs>(self, args: &StartArgs) -> HandlesAndErrors<StaticPlugins::StartArgs>
    where
        StaticPlugins: MultipleStaticPlugins<StartArgs = StartArgs>,
    {
        let names = self.ids;
        let (running_plugins, errors) =
            StaticPlugins::start(&mut self.should_run.into_iter(), args);
        (
            PluginsHandles {
                handles: StaticPluginsHandles {
                    running_plugins,
                    names,
                },
                dynamic_plugins: Vec::new(),
            },
            errors,
        )
    }
}

pub struct StaticPluginsHandles {
    running_plugins: Vec<(String, RunningPlugin)>,
    names: Vec<String>,
}

impl AsRef<Vec<(String, RunningPlugin)>> for StaticPluginsHandles {
    fn as_ref(&self) -> &Vec<(String, RunningPlugin)> {
        &self.running_plugins
    }
}

impl AsMut<Vec<(String, RunningPlugin)>> for StaticPluginsHandles {
    fn as_mut(&mut self) -> &mut Vec<(String, RunningPlugin)> {
        &mut self.running_plugins
    }
}

pub struct DynamicLoader<Statics: MultipleStaticPlugins> {
    _static: Statics,
    loader: LibLoader,
    dynamic_plugins: Vec<DynamicPlugin<Statics::StartArgs>>,
}

impl<StaticPlugins: MultipleStaticPlugins> DynamicLoader<StaticPlugins> {
    pub fn load_plugin_by_name(&mut self, name: String) -> ZResult<()> {
        let (lib, p) = unsafe { self.loader.search_and_load(&format!("zplugin_{}", &name))? };
        let plugin = DynamicPlugin::new(name.clone(), lib, p).unwrap_or_else(|e| panic!("Wrong PluginVTable version, your {} doesn't appear to be compatible with this version of Zenoh (vtable versions: plugin v{}, zenoh v{})", name, e.map_or_else(|| "UNKNWON".to_string(), |e| e.to_string()), PLUGIN_VTABLE_VERSION));
        self.dynamic_plugins.push(plugin);
        Ok(())
    }
    pub fn load_plugin_by_paths<P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: String,
        paths: &[P],
    ) -> ZResult<()> {
        for path in paths {
            let path = path.as_ref();
            match unsafe { LibLoader::load_file(path) } {
                Ok((lib, p)) => {
                    self.dynamic_plugins.push(
                        DynamicPlugin::new(name.clone(), lib, p).unwrap_or_else(|e| panic!("Wrong PluginVTable version, your {} doesn't appear to be compatible with this version of Zenoh (vtable versions: plugin v{}, zenoh v{})", name, e.map_or_else(|| "UNKNWON".to_string(), |e| e.to_string()), PLUGIN_VTABLE_VERSION)));
                    return Ok(());
                }
                Err(e) => log::warn!("Plugin '{}' load fail at {}: {}", &name, path, e),
            }
        }
        bail!("Plugin '{}' not found in {:?}", name, &paths)
    }
    pub fn search_and_load_plugins(mut self, prefix: Option<&str>) -> Self {
        let libs = unsafe { self.loader.load_all_with_prefix(prefix) };
        self.dynamic_plugins.extend(
            libs.into_iter()
                .map(|(lib, path, name)| (DynamicPlugin::new(name, lib, path.clone()), path))
                .filter_map(|(p, path)| match p {
                    Ok(p) => Some(p),
                    Err(e) => {
                        log::debug!("{:?} failed to load: {:?}", path, e);
                        None
                    }
                }),
        );
        self
    }

    pub fn build(self) -> DynamicStarter<StaticPlugins>
    where
        StaticPlugins: MultipleStaticPlugins,
    {
        let (static_launcher, mut compatibilites) = self._static.build();
        let mut incompatibilities = Vec::new();
        let dynamic_plugins = self.dynamic_plugins;
        for plugin in &dynamic_plugins {
            match plugin.is_compatible_with(&compatibilites) {
                Ok(c) => {
                    compatibilites.push(c);
                    incompatibilities.push(None);
                }
                Err(i) => incompatibilities.push(Some(i)),
            }
        }
        DynamicStarter {
            static_launcher,
            dynamic_plugins,
            incompatibilities,
        }
    }
}

pub struct DynamicStarter<StaticPlugins: MultipleStaticPlugins> {
    static_launcher: StaticLauncher<StaticPlugins>,
    dynamic_plugins: Vec<DynamicPlugin<StaticPlugins::StartArgs>>,
    incompatibilities: Vec<Option<Incompatibility>>,
}

impl<StaticPlugins: MultipleStaticPlugins> DynamicStarter<StaticPlugins> {
    pub fn start(
        self,
        args: &StaticPlugins::StartArgs,
    ) -> HandlesAndErrors<StaticPlugins::StartArgs> {
        use std::cell::UnsafeCell;
        let (mut handle, errors) = self.static_launcher.start(args);
        let errors = UnsafeCell::new(errors);
        let dynamic_plugins = self.dynamic_plugins;
        for plugin in dynamic_plugins
            .iter()
            .zip(self.incompatibilities.into_iter())
            .filter_map(|(p, i)| match i {
                Some(i) => {
                    unsafe { &mut *errors.get() }.push(Box::new(i));
                    None
                }
                None => Some(p),
            })
        {
            let name = plugin.name.clone();
            match plugin.start(args) {
                Ok(p) => handle.handles.running_plugins.push((name, p)),
                Err(e) => unsafe { &mut *errors.get() }.push(e),
            }
        }
        handle.dynamic_plugins.extend(dynamic_plugins);
        (handle, errors.into_inner())
    }
}

pub struct PluginsHandles<StartArgs> {
    handles: StaticPluginsHandles,
    dynamic_plugins: Vec<DynamicPlugin<StartArgs>>,
}

pub struct PluginDescription<'l> {
    pub name: &'l str,
    pub path: &'l str,
}

impl<B> PluginsHandles<B> {
    pub fn plugins(&self) -> Vec<PluginDescription> {
        self.handles
            .names
            .iter()
            .map(|name| PluginDescription {
                name,
                path: "<static-linking>",
            })
            .chain(self.dynamic_plugins.iter().map(|p| PluginDescription {
                name: &p.name,
                path: p.path.to_str().unwrap(),
            }))
            .collect()
    }
    pub fn running_plugins(&self) -> &[(String, RunningPlugin)] {
        &self.handles.running_plugins
    }
}

impl<B> AsRef<Vec<(String, RunningPlugin)>> for PluginsHandles<B> {
    fn as_ref(&self) -> &Vec<(String, RunningPlugin)> {
        self.handles.as_ref()
    }
}

impl<B> AsMut<Vec<(String, RunningPlugin)>> for PluginsHandles<B> {
    fn as_mut(&mut self) -> &mut Vec<(String, RunningPlugin)> {
        self.handles.as_mut()
    }
}

pub struct DynamicPlugin<StartArgs> {
    _lib: Library,
    vtable: PluginVTable<StartArgs>,
    pub name: String,
    pub path: PathBuf,
}

impl<StartArgs> DynamicPlugin<StartArgs> {
    fn new(name: String, lib: Library, path: PathBuf) -> Result<Self, Option<PluginVTableVersion>> {
        let load_plugin = unsafe {
            lib.get::<fn(PluginVTableVersion) -> LoadPluginResult<StartArgs>>(b"load_plugin")
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

    fn is_compatible_with(&self, others: &[PluginId]) -> Result<PluginId, Incompatibility> {
        self.vtable.is_compatible_with(others)
    }

    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin> {
        self.vtable.start(&self.name, args)
    }
}
