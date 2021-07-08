use crate::vtable::*;
use crate::*;
use libloading::Library;
use std::path::PathBuf;
use zenoh_util::core::ZResult;
use zenoh_util::LibLoader;

type ReqsAndCompats<Requirements> = (
    Requirements,
    Vec<Compatibility>,
    Vec<Option<Incompatibility>>,
);

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
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>);

    fn __len(cur: usize) -> usize;
    fn len() -> usize {
        Self::__len(0)
    }
    fn get_requirements(self) -> (StaticLauncher<Self>, Self::Requirements, Vec<Compatibility>) {
        let (reqs, ids, should_run) = Self::merge_requirements_and_conflicts();
        (StaticLauncher::new(self, should_run), reqs, ids)
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
        it: &mut std::vec::IntoIter<Option<Incompatibility>>,
        runtime: &StartArgs,
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>) {
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
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>) {
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
}

impl<StaticPlugins> StaticLauncher<StaticPlugins> {
    fn new(static_plugins: StaticPlugins, should_run: Vec<Option<Incompatibility>>) -> Self {
        StaticLauncher {
            _static_plugins: static_plugins,
            should_run,
        }
    }

    pub fn incompatibilities(&self) -> impl Iterator<Item = &Incompatibility> {
        self.should_run.iter().filter_map(Option::as_ref)
    }

    pub fn start<StartArgs>(self, args: &StartArgs) -> (StaticPluginsStopper, Vec<Box<dyn Error>>)
    where
        StaticPlugins: MultipleStaticPlugins<StartArgs = StartArgs>,
    {
        let (stoppers, errors) = StaticPlugins::start(&mut self.should_run.into_iter(), args);
        (StaticPluginsStopper { stoppers }, errors)
    }
}

pub struct StaticPluginsStopper {
    stoppers: Vec<Box<dyn PluginStopper>>,
}

impl StaticPluginsStopper {
    pub fn stop(self) {
        for stopper in self.stoppers {
            stopper.stop()
        }
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
    pub fn search_and_load_plugins(mut self) -> Self {
        let libs = unsafe { self.loader.load_all_with_prefix(Some(&"*PLUGIN_PREFIX")) };
        self.dynamic_plugins.extend(
            libs.into_iter()
                .map(|(lib, path, name)| DynamicPlugin::new(name, lib, path).map_err(|_| todo!()))
                .filter_map(ZResult::ok),
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
    ) -> (DynamicPluginsStopper<StaticPlugins>, Vec<Box<dyn Error>>) {
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
                Ok(p) => stopper.stoppers.push(p),
                Err(e) => unsafe { &mut *errors.get() }.push(e),
            }
        }
        (
            DynamicPluginsStopper {
                stopper,
                _dynamic_plugins: dynamic_plugins,
            },
            errors.into_inner(),
        )
    }
}

pub struct DynamicPluginsStopper<StaticPlugins: MultipleStaticPlugins> {
    stopper: StaticPluginsStopper,
    _dynamic_plugins: Vec<DynamicPlugin<StaticPlugins::Requirements, StaticPlugins::StartArgs>>,
}

impl<StaticPlugins: MultipleStaticPlugins> DynamicPluginsStopper<StaticPlugins> {
    pub fn stop(self) {
        self.stopper.stop()
    }
}

// impl<StaticPlugins, Runtime> PluginsLoader<StaticPlugins, Runtime> {
//     pub fn load_plugins(mut self, paths: &[String]) -> Self {
//     }

//     pub fn get_expected_args(
//         self,
//     ) -> (
//         Vec<Arg<'static, 'static>>,
//         PluginsIniter<StaticPlugins, Runtime>,
//     )
//     where
//         StaticPlugins: MultipleStaticPlugins<Runtime>,
//     {
//         let (mut args, mut compats, mut should_init) =
//             StaticPlugins::get_expected_args_and_compatibility();
//         for plugin in &self.dynamic_plugins {
//             match plugin.is_compatible_with(&compats) {
//                 Ok(c) => {
//                     compats.push(c);
//                     should_init.push(Ok(()));
//                     args.extend(plugin.get_expected_args());
//                 }
//                 Err(i) => should_init.push(Err(i)),
//             }
//         }
//         (
//             args,
//             PluginsIniter {
//                 loader: self,
//                 should_init,
//             },
//         )
//     }
// }

// pub struct PluginsIniter<StaticPlugins, Runtime> {
//     loader: PluginsLoader<StaticPlugins, Runtime>,
//     should_init: Vec<Result<(), Incompatibility>>,
// }

// impl<StaticPlugins, Runtime> PluginsIniter<StaticPlugins, Runtime> {
//     pub fn init(
//         self,
//         args: &ArgMatches,
//     ) -> (PluginsStarter<StaticPlugins, Runtime>, Vec<Incompatibility>)
//     where
//         StaticPlugins: MultipleStaticPlugins<Runtime>,
//     {
//         let static_starters = StaticPlugins::init(args, &self.should_init);
//         let mut dynamic_starters = Vec::with_capacity(self.loader.dynamic_plugins.len());
//         for plugin in self
//             .loader
//             .dynamic_plugins
//             .iter()
//             .zip(&self.should_init[StaticPlugins::len()..])
//             .filter_map(|(p, s)| match s {
//                 Ok(_) => Some(p),
//                 Err(_) => None,
//             })
//         {
//             dynamic_starters.push(plugin.init(args))
//         }
//         (
//             PluginsStarter {
//                 static_starters,
//                 _static_plugins: self.loader._static_plugins,
//                 dynamic_plugins: self.loader.dynamic_plugins,
//                 dynamic_starters,
//             },
//             self.should_init
//                 .into_iter()
//                 .filter_map(Result::err)
//                 .collect(),
//         )
//     }
// }

// pub struct PluginsStarter<StaticPlugins, Runtime> {
//     static_starters: InitResultVec,
//     _static_plugins: StaticPlugins,
//     dynamic_starters: InitResultVec,
//     dynamic_plugins: Vec<DynamicPlugin<Runtime>>,
// }

// impl<StaticPlugins: MultipleStaticPlugins<Runtime>, Runtime>
//     PluginsStarter<StaticPlugins, Runtime>
// {
//     pub fn start(self, runtime: &Runtime) -> (PluginsStopper<Runtime>, Vec<Box<dyn Error>>) {
//         let (mut stoppers, mut errors) =
//             StaticPlugins::start(&mut self.static_starters.into_iter(), runtime);
//         stoppers.extend(
//             self.dynamic_starters
//                 .into_iter()
//                 .zip(self.dynamic_plugins.iter())
//                 .filter_map(|(s, p)| match s {
//                     Ok(mut s) => Some(unsafe { p.vtable.start(thin_ptr(&mut s), runtime.clone()) }),
//                     Err(e) => {
//                         errors.push(e);
//                         None
//                     }
//                 }),
//         );
//         (
//             PluginsStopper {
//                 stoppers,
//                 dynamic_plugins: self.dynamic_plugins,
//             },
//             errors,
//         )
//     }
// }

// pub struct PluginsStopper<Runtime> {
//     stoppers: Vec<Box<dyn PluginStopper>>,
//     dynamic_plugins: Vec<DynamicPlugin<Runtime>>,
// }

// impl<Runtime> PluginsStopper<Runtime> {
//     pub fn plugins(&self) -> &[DynamicPlugin<Runtime>] {
//         &self.dynamic_plugins
//     }

//     pub fn stop(self) {
//         for stopper in self.stoppers {
//             stopper.stop();
//         }
//     }
// }

pub struct DynamicPlugin<Requirements, StartArgs> {
    lib: Library,
    vtable: PluginVTable<Requirements, StartArgs>,
    pub name: String,
    pub path: PathBuf,
}

impl<Requirements, StartArgs> DynamicPlugin<Requirements, StartArgs> {
    fn new(name: String, lib: Library, path: PathBuf) -> Result<Self, ()> {
        let load_plugin = unsafe {
            lib.get::<fn(PluginVTableVersion) -> LoadPluginResult<Requirements, StartArgs>>(
                b"load_plugin",
            )
            .map_err(|_| ())?
        };
        match load_plugin(PLUGIN_VTABLE_VERSION) {
            Ok(vtable) => Ok(DynamicPlugin {
                name,
                lib,
                vtable,
                path,
            }),
            Err(plugin_version) => todo!(),
        }
    }

    fn is_compatible_with(
        &self,
        others: &[Compatibility],
    ) -> Result<Compatibility, Incompatibility> {
        self.vtable.is_compatible_with(others)
    }

    fn get_requirements(&self) -> Requirements {
        self.vtable.get_requirements()
    }

    fn start(&self, args: &StartArgs) -> Result<Box<dyn PluginStopper>, Box<dyn Error>> {
        self.vtable.start(args)
    }
}
