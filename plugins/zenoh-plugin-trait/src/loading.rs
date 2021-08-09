use crate::vtable::*;
use crate::*;
use libloading::Library;
use std::path::PathBuf;
use zenoh_util::core::ZResult;
use zenoh_util::LibLoader;

type ReqsAndCompats<Requirements> = (Requirements, Vec<PluginId>, Vec<Option<Incompatibility>>);

pub struct StaticPlugins<A, B, C, D> {
    a: std::marker::PhantomData<A>,
    b: std::marker::PhantomData<B>,
    c: std::marker::PhantomData<C>,
    d: std::marker::PhantomData<D>,
}
impl<C, D> StaticPlugins<(), (), C, D> {
    #[inline(always)]
    pub fn builder() -> Self {
        Self::new()
    }
}
impl<A, B, C, D> StaticPlugins<A, B, C, D> {
    #[inline(always)]
    fn new() -> Self {
        StaticPlugins {
            a: Default::default(),
            b: Default::default(),
            c: Default::default(),
            d: Default::default(),
        }
    }
    #[inline(always)]
    pub fn add_static<E>(self) -> StaticPlugins<E, Self, C, D> {
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

impl<A, B, C, D> Default for StaticPlugins<A, B, C, D> {
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

pub trait MultipleStaticPlugins: Sized {
    type Requirements: MergeRequirements;
    type StartArgs;
    fn merge_requirements_and_conflicts() -> ReqsAndCompats<Self::Requirements>;

    fn start(
        it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        args: &Self::StartArgs,
    ) -> (Vec<BoxedAny>, Vec<Box<dyn Error>>);

    fn __len(cur: usize) -> usize;
    fn len() -> usize {
        Self::__len(0)
    }
    fn get_requirements(self) -> (StaticLauncher<Self>, Self::Requirements, Vec<PluginId>) {
        let (reqs, ids, should_run) = Self::merge_requirements_and_conflicts();
        (StaticLauncher::new(self, should_run, &ids), reqs, ids)
    }
}
impl<Requirements: MergeRequirements, StartArgs> MultipleStaticPlugins
    for StaticPlugins<(), (), Requirements, StartArgs>
{
    type Requirements = Requirements;
    type StartArgs = StartArgs;
    #[inline(always)]
    fn merge_requirements_and_conflicts() -> ReqsAndCompats<Requirements> {
        (Requirements::zero(), vec![], vec![])
    }

    fn __len(cur: usize) -> usize {
        cur
    }

    fn start(
        _it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        _args: &StartArgs,
    ) -> (Vec<BoxedAny>, Vec<Box<dyn Error>>) {
        (vec![], vec![])
    }
}
impl<
        StartArgs,
        Requirements: MergeRequirements,
        A: Plugin<Requirements = Requirements, StartArgs = StartArgs>,
        B: MultipleStaticPlugins<Requirements = Requirements, StartArgs = StartArgs>,
    > MultipleStaticPlugins for StaticPlugins<A, B, Requirements, StartArgs>
{
    type Requirements = Requirements;
    type StartArgs = StartArgs;
    #[inline(always)]
    fn merge_requirements_and_conflicts() -> ReqsAndCompats<Requirements> {
        let (mut args, mut compats, mut should_init) = B::merge_requirements_and_conflicts();
        match A::is_compatible_with(&compats) {
            Ok(c) => {
                compats.push(c);
                should_init.push(None);
                args = args.merge(A::get_requirements());
            }
            Err(i) => {
                should_init.push(Some(i));
            }
        }
        (args, compats, should_init)
    }

    fn __len(cur: usize) -> usize {
        B::__len(cur + 1)
    }

    #[inline(always)]
    fn start(
        it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        args: &StartArgs,
    ) -> (Vec<BoxedAny>, Vec<Box<dyn Error>>) {
        let (mut ok, mut err) = B::start(it, args);
        match it.next().unwrap() {
            None => match A::start(args) {
                Ok(s) => ok.push(s),
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

type HandlesAndErrors<Requirements, StartArgs> =
    (PluginsHandles<Requirements, StartArgs>, Vec<Box<dyn Error>>);

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

    pub fn start<StartArgs>(
        self,
        args: &StartArgs,
    ) -> HandlesAndErrors<StaticPlugins::Requirements, StaticPlugins::StartArgs>
    where
        StaticPlugins: MultipleStaticPlugins<StartArgs = StartArgs>,
    {
        let names = self.ids;
        let (stoppers, errors) = StaticPlugins::start(&mut self.should_run.into_iter(), args);
        (
            PluginsHandles {
                stopper: StaticPluginsHandles { stoppers, names },
                dynamic_plugins: Vec::new(),
            },
            errors,
        )
    }
}

pub struct StaticPluginsHandles {
    stoppers: Vec<BoxedAny>,
    names: Vec<String>,
}

impl AsRef<Vec<BoxedAny>> for StaticPluginsHandles {
    fn as_ref(&self) -> &Vec<BoxedAny> {
        &self.stoppers
    }
}

impl AsMut<Vec<BoxedAny>> for StaticPluginsHandles {
    fn as_mut(&mut self) -> &mut Vec<BoxedAny> {
        &mut self.stoppers
    }
}

pub struct DynamicLoader<Statics: MultipleStaticPlugins> {
    _static: Statics,
    loader: LibLoader,
    dynamic_plugins: Vec<DynamicPlugin<Statics::Requirements, Statics::StartArgs>>,
}

impl<StaticPlugins: MultipleStaticPlugins> DynamicLoader<StaticPlugins> {
    pub fn load_plugins<P: AsRef<str>>(mut self, paths: &[P], plugin_prefix: &str) -> Self {
        let prefix = format!("{}{}", *zenoh_util::LIB_PREFIX, plugin_prefix);
        let suffix = &*zenoh_util::LIB_SUFFIX;
        self.dynamic_plugins.extend(
            paths
                .iter()
                .map(
                    |path| -> ZResult<
                        DynamicPlugin<StaticPlugins::Requirements, StaticPlugins::StartArgs>,
                    > {
                        let (lib, p) = unsafe { LibLoader::load_file(path.as_ref())? };
                        let filename = p.file_name().unwrap().to_str().unwrap();
                        let name = if filename.starts_with(&prefix)
                            && path.as_ref().ends_with(suffix)
                        {
                            filename[(prefix.len())..(filename.len() - suffix.len())].to_string()
                        } else {
                            filename.to_string()
                        };
                        DynamicPlugin::new(name, lib, p).map_err(|_| todo!())
                    },
                )
                .filter_map(ZResult::ok),
        );
        self
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

    pub fn get_requirements(self) -> (DynamicStarter<StaticPlugins>, StaticPlugins::Requirements)
    where
        StaticPlugins: MultipleStaticPlugins,
    {
        let (static_launcher, mut requirements, mut compatibilites) =
            self._static.get_requirements();
        let mut incompatibilities = Vec::new();
        let dynamic_plugins = self.dynamic_plugins;
        for plugin in &dynamic_plugins {
            match plugin.is_compatible_with(&compatibilites) {
                Ok(c) => {
                    requirements = requirements.merge(plugin.get_requirements());
                    compatibilites.push(c);
                    incompatibilities.push(None);
                }
                Err(i) => incompatibilities.push(Some(i)),
            }
        }
        (
            DynamicStarter {
                static_launcher,
                dynamic_plugins,
                incompatibilities,
            },
            requirements,
        )
    }
}

pub struct DynamicStarter<StaticPlugins: MultipleStaticPlugins> {
    static_launcher: StaticLauncher<StaticPlugins>,
    dynamic_plugins: Vec<DynamicPlugin<StaticPlugins::Requirements, StaticPlugins::StartArgs>>,
    incompatibilities: Vec<Option<Incompatibility>>,
}

impl<StaticPlugins: MultipleStaticPlugins> DynamicStarter<StaticPlugins> {
    pub fn start(
        self,
        args: &StaticPlugins::StartArgs,
    ) -> HandlesAndErrors<StaticPlugins::Requirements, StaticPlugins::StartArgs> {
        use std::cell::UnsafeCell;
        let (mut stopper, errors) = self.static_launcher.start(args);
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
            match plugin.start(args) {
                Ok(p) => stopper.stopper.stoppers.push(p),
                Err(e) => unsafe { &mut *errors.get() }.push(e),
            }
        }
        stopper.dynamic_plugins.extend(dynamic_plugins);
        (stopper, errors.into_inner())
    }
}

pub struct PluginsHandles<Requirements, StartArgs> {
    stopper: StaticPluginsHandles,
    dynamic_plugins: Vec<DynamicPlugin<Requirements, StartArgs>>,
}

pub struct PluginDescription<'l> {
    pub name: &'l str,
    pub path: &'l str,
}

impl<A, B> PluginsHandles<A, B> {
    pub fn plugins(&self) -> Vec<PluginDescription> {
        self.stopper
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
}

impl<A, B> AsRef<Vec<BoxedAny>> for PluginsHandles<A, B> {
    fn as_ref(&self) -> &Vec<BoxedAny> {
        self.stopper.as_ref()
    }
}

impl<A, B> AsMut<Vec<BoxedAny>> for PluginsHandles<A, B> {
    fn as_mut(&mut self) -> &mut Vec<BoxedAny> {
        self.stopper.as_mut()
    }
}

pub struct DynamicPlugin<Requirements, StartArgs> {
    _lib: Library,
    vtable: PluginVTable<Requirements, StartArgs>,
    pub name: String,
    pub path: PathBuf,
}

impl<Requirements, StartArgs> DynamicPlugin<Requirements, StartArgs> {
    fn new(name: String, lib: Library, path: PathBuf) -> Result<Self, Option<PluginVTableVersion>> {
        let load_plugin = unsafe {
            lib.get::<fn(PluginVTableVersion) -> LoadPluginResult<Requirements, StartArgs>>(
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

    fn is_compatible_with(&self, others: &[PluginId]) -> Result<PluginId, Incompatibility> {
        self.vtable.is_compatible_with(others)
    }

    fn get_requirements(&self) -> Requirements {
        self.vtable.get_requirements()
    }

    fn start(&self, args: &StartArgs) -> Result<BoxedAny, Box<dyn Error>> {
        self.vtable.start(args)
    }
}
