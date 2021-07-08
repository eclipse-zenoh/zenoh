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
use clap::{Arg, ArgMatches};
use libloading::{Library, Symbol};
use log::{debug, trace, warn};
use std::error::Error;
use std::path::PathBuf;
use zenoh_plugin_trait::{prelude::*, Compatibility, Incompatibility};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zconfigurable, zerror, LibLoader};
zconfigurable! {
    static ref PLUGIN_PREFIX: String = "zplugin_".to_string();
}

pub struct StaticPlugins<A, B> {
    a: std::marker::PhantomData<A>,
    b: std::marker::PhantomData<B>,
}
type ArgsAndCompats = (
    Vec<Arg<'static, 'static>>,
    Vec<Compatibility>,
    Vec<Result<(), Incompatibility>>,
);
type InitResultVec = Vec<Result<Box<dyn std::any::Any>, Box<dyn Error>>>;

impl StaticPlugins<(), ()> {
    #[inline(always)]
    pub fn builder() -> Self {
        Self::new()
    }
}
impl<A, B> StaticPlugins<A, B> {
    #[inline(always)]
    pub fn new() -> Self {
        StaticPlugins {
            a: Default::default(),
            b: Default::default(),
        }
    }
    #[inline(always)]
    pub fn add_static<C>(self) -> StaticPlugins<C, Self> {
        StaticPlugins::new()
    }

    pub fn into_dynamic(self, lib_loader: LibLoader) -> PluginsLoader<Self> {
        PluginsLoader {
            lib_loader,
            dynamic_plugins: vec![],
            _static_plugins: self,
        }
    }
}

impl<A, B> Default for StaticPlugins<A, B> {
    fn default() -> Self {
        Self::new()
    }
}
pub trait StaticPluginsLoad {
    fn get_expected_args_and_compatibility() -> ArgsAndCompats;

    fn init(args: &ArgMatches, compats: &[Result<(), Incompatibility>]) -> InitResultVec;

    fn start(
        it: &mut std::vec::IntoIter<Result<Box<dyn std::any::Any>, Box<dyn Error>>>,
        runtime: &Runtime,
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>);

    fn __len(cur: usize) -> usize;
    fn len() -> usize {
        Self::__len(0)
    }
}
impl StaticPluginsLoad for StaticPlugins<(), ()> {
    #[inline(always)]
    fn get_expected_args_and_compatibility() -> ArgsAndCompats {
        (vec![], vec![], vec![Ok(())])
    }

    #[inline(always)]
    fn init(_args: &ArgMatches, _compats: &[Result<(), Incompatibility>]) -> InitResultVec {
        vec![]
    }

    fn __len(cur: usize) -> usize {
        cur
    }

    fn start(
        _it: &mut std::vec::IntoIter<Result<Box<dyn std::any::Any>, Box<dyn Error>>>,
        runtime: &Runtime,
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>) {
        (vec![], vec![])
    }
}
impl<A: Plugin<Runtime = Runtime>, B: StaticPluginsLoad> StaticPluginsLoad for StaticPlugins<A, B> {
    #[inline(always)]
    fn get_expected_args_and_compatibility() -> ArgsAndCompats {
        let (mut args, mut compats, mut should_init) = B::get_expected_args_and_compatibility();
        match A::is_compatible_with(&compats) {
            Ok(c) => {
                compats.push(c);
                should_init.push(Ok(()));
                args.extend(A::get_expected_args());
            }
            Err(i) => {
                should_init.push(Err(i));
            }
        }
        (args, compats, should_init)
    }

    #[inline(always)]
    fn init(args: &ArgMatches, compats: &[Result<(), Incompatibility>]) -> InitResultVec {
        let mut r = B::init(args, &compats[1..]);
        r.push(A::box_init(args));
        r
    }

    fn __len(cur: usize) -> usize {
        B::__len(cur + 1)
    }

    fn start(
        it: &mut std::vec::IntoIter<Result<Box<dyn std::any::Any>, Box<dyn Error>>>,
        runtime: &Runtime,
    ) -> (Vec<Box<dyn PluginStopper>>, Vec<Box<dyn Error>>) {
        let (mut ok, mut err) = B::start(it, runtime);
        match it.next().unwrap() {
            Ok(mut s) => unsafe { ok.push(A::do_start(thin_ptr(&mut s), runtime.clone())) },
            Err(e) => err.push(e),
        }
        (ok, err)
    }
}

pub struct PluginsLoader<StaticPlugins> {
    lib_loader: LibLoader,
    dynamic_plugins: Vec<DynamicPlugin>,
    _static_plugins: StaticPlugins,
}

impl<StaticPlugins> PluginsLoader<StaticPlugins> {
    pub fn load_plugins(mut self, paths: &[String]) -> Self {
        let prefix = format!("{}{}", *zenoh_util::LIB_PREFIX, *PLUGIN_PREFIX);
        let suffix = &*zenoh_util::LIB_SUFFIX;
        self.dynamic_plugins.extend(
            paths
                .iter()
                .map(|path| -> ZResult<DynamicPlugin> {
                    let (lib, p) = unsafe { LibLoader::load_file(path)? };
                    let filename = p.file_name().unwrap().to_str().unwrap();
                    let name = if filename.starts_with(&prefix) && path.ends_with(suffix) {
                        filename[(prefix.len())..(filename.len() - suffix.len())].to_string()
                    } else {
                        filename.to_string()
                    };
                    DynamicPlugin::new(name, lib, p).map_err(|_| todo!())
                })
                .filter_map(ZResult::ok),
        );
        self
    }

    pub fn search_and_load_plugins(mut self) -> Self {
        let libs = unsafe { self.lib_loader.load_all_with_prefix(Some(&*PLUGIN_PREFIX)) };
        self.dynamic_plugins.extend(
            libs.into_iter()
                .map(|(lib, path, name)| DynamicPlugin::new(name, lib, path).map_err(|_| todo!()))
                .filter_map(ZResult::ok),
        );
        self
    }

    pub fn get_expected_args(self) -> (Vec<Arg<'static, 'static>>, PluginsIniter<StaticPlugins>)
    where
        StaticPlugins: StaticPluginsLoad,
    {
        let (mut args, mut compats, mut should_init) =
            StaticPlugins::get_expected_args_and_compatibility();
        for plugin in &self.dynamic_plugins {
            match plugin.is_compatible_with(&compats) {
                Ok(c) => {
                    compats.push(c);
                    should_init.push(Ok(()));
                    args.extend(plugin.get_expected_args());
                }
                Err(i) => should_init.push(Err(i)),
            }
        }
        (
            args,
            PluginsIniter {
                loader: self,
                should_init,
            },
        )
    }
}

pub struct PluginsIniter<StaticPlugins> {
    loader: PluginsLoader<StaticPlugins>,
    should_init: Vec<Result<(), Incompatibility>>,
}

impl<StaticPlugins> PluginsIniter<StaticPlugins> {
    pub fn init(self, args: &ArgMatches) -> (PluginsStarter<StaticPlugins>, Vec<Incompatibility>)
    where
        StaticPlugins: StaticPluginsLoad,
    {
        let static_starters = StaticPlugins::init(args, &self.should_init);
        let mut dynamic_starters = Vec::with_capacity(self.loader.dynamic_plugins.len());
        for plugin in self
            .loader
            .dynamic_plugins
            .iter()
            .zip(&self.should_init[StaticPlugins::len()..])
            .filter_map(|(p, s)| match s {
                Ok(_) => Some(p),
                Err(_) => None,
            })
        {
            dynamic_starters.push(plugin.init(args))
        }
        (
            PluginsStarter {
                static_starters,
                _static_plugins: self.loader._static_plugins,
                dynamic_plugins: self.loader.dynamic_plugins,
                dynamic_starters,
            },
            self.should_init
                .into_iter()
                .filter_map(Result::err)
                .collect(),
        )
    }
}

pub struct PluginsStarter<StaticPlugins> {
    static_starters: InitResultVec,
    _static_plugins: StaticPlugins,
    dynamic_starters: InitResultVec,
    dynamic_plugins: Vec<DynamicPlugin>,
}

fn thin_ptr(fat: &mut dyn std::any::Any) -> *mut () {
    (fat as *mut dyn std::any::Any).cast()
}

impl<StaticPlugins: StaticPluginsLoad> PluginsStarter<StaticPlugins> {
    pub fn start(self, runtime: &Runtime) -> (PluginsStopper, Vec<Box<dyn Error>>) {
        let (mut stoppers, mut errors) =
            StaticPlugins::start(&mut self.static_starters.into_iter(), runtime);
        stoppers.extend(
            self.dynamic_starters
                .into_iter()
                .zip(self.dynamic_plugins.iter())
                .filter_map(|(s, p)| match s {
                    Ok(mut s) => Some(unsafe { p.vtable.start(thin_ptr(&mut s), runtime.clone()) }),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                }),
        );
        (
            PluginsStopper {
                stoppers,
                dynamic_plugins: self.dynamic_plugins,
            },
            errors,
        )
    }
}

pub struct PluginsStopper {
    stoppers: Vec<Box<dyn PluginStopper>>,
    dynamic_plugins: Vec<DynamicPlugin>,
}

impl PluginsStopper {
    pub fn plugins(&self) -> &[DynamicPlugin] {
        &self.dynamic_plugins
    }

    pub fn stop(self) {
        for stopper in self.stoppers {
            stopper.stop();
        }
    }
}

pub struct DynamicPlugin {
    lib: Library,
    vtable: PluginVTable<Runtime>,
    pub name: String,
    pub path: PathBuf,
}

impl DynamicPlugin {
    fn new(name: String, lib: Library, path: PathBuf) -> Result<Self, ()> {
        let load_plugin = unsafe {
            lib.get::<fn(PluginVTableVersion) -> LoadPluginResult<Runtime>>(b"load_plugin")
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

    fn get_expected_args(&self) -> Vec<Arg<'static, 'static>> {
        self.vtable.get_expected_args()
    }

    fn init(&self, args: &ArgMatches) -> Result<Box<dyn std::any::Any>, Box<dyn Error>> {
        self.vtable.init(args)
    }
}
