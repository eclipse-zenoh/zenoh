//
// Copyright (c) 2024 ZettaScale Technology
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
use core::panic;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    future::Future,
    ops::Deref,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::Duration,
};
use tokio::runtime::{Handle, Runtime, RuntimeFlavor};
use zenoh_result::{bail, ZError, ZResult as Result};
use zenoh_runtime_derive::{ConfigureZRuntime, GenericRuntimeParam};

const ZENOH_RUNTIME_ENV: &str = "ZENOH_RUNTIME";

trait DefaultParam {
    fn param() -> RuntimeParam;
}

#[derive(Deserialize, Debug, GenericRuntimeParam)]
#[serde(deny_unknown_fields, default)]
pub struct RuntimeParam {
    pub worker_threads: usize,
    pub max_blocking_threads: usize,
    pub handover: Option<ZRuntime>,
}

impl Default for RuntimeParam {
    fn default() -> Self {
        Self {
            worker_threads: 1,
            max_blocking_threads: 50,
            handover: None,
        }
    }
}

pub trait RuntimeParamTrait {
    fn param(&self) -> &RuntimeParam;
}

impl RuntimeParam {
    pub fn build(&self, zrt: ZRuntime) -> Result<Runtime> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(self.worker_threads)
            .max_blocking_threads(self.max_blocking_threads)
            .enable_io()
            .enable_time()
            .thread_name_fn(move || {
                let id = ZRUNTIME_INDEX
                    .get(&zrt)
                    .unwrap()
                    .fetch_add(1, Ordering::SeqCst);
                format!("{}-{}", zrt, id)
            })
            .build()?;
        Ok(rt)
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, ConfigureZRuntime, Deserialize)]
pub enum ZRuntime {
    #[serde(rename = "app")]
    #[param(worker_threads = 1, max_blocking_threads = 1)]
    Application,

    #[serde(rename = "acc")]
    #[param(worker_threads = 1, handover = Some(ZRuntime::Application))]
    Acceptor,

    #[serde(rename = "tx")]
    #[param(worker_threads = 1, max_blocking_threads = 1)]
    TX,

    #[serde(rename = "rx")]
    #[param(worker_threads = 1, handover = Some(ZRuntime::Application))]
    RX,

    #[serde(rename = "net")]
    #[param(worker_threads = 1, handover = Some(ZRuntime::Application))]
    Net,
}

impl FromStr for ZRuntime {
    type Err = ZError;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        if let Some(zrt) = ZRuntime::iter().find(|zrt| s == zrt.to_string()) {
            Ok(zrt)
        } else {
            bail!("Invalid ZRuntime name: {s}")
        }
    }
}

impl ZRuntime {
    pub fn block_in_place<F, R>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        if let Ok(handle) = Handle::try_current() {
            if handle.runtime_flavor() == RuntimeFlavor::CurrentThread {
                panic!("Zenoh runtime doesn't support Tokio's current thread scheduler. Please use multi thread scheduler instead, e.g. a multi thread scheduler with one worker thread: `#[tokio::main(flavor = \"multi_thread\", worker_threads = 1)]`");
            }
        }
        tokio::task::block_in_place(move || self.block_on(f))
    }
}

impl Deref for ZRuntime {
    type Target = Handle;
    fn deref(&self) -> &Self::Target {
        ZRUNTIME_POOL.get(self)
    }
}

lazy_static! {
    // pub static ref ZRUNTIME_CONFIG: ZRuntimeConfig = ZRuntimeConfig::from_env();
    pub static ref ZRUNTIME_POOL: ZRuntimePool = ZRuntimePool::new();
    pub static ref ZRUNTIME_INDEX: HashMap<ZRuntime, AtomicUsize> = ZRuntime::iter().map(|zrt| (zrt, AtomicUsize::new(0))).collect();
}

// To drop the data mannually since Rust does not drop static variables.
pub extern "C" fn cleanup() {
    unsafe {
        std::mem::drop((ZRUNTIME_POOL.deref() as *const ZRuntimePool).read());
        std::mem::drop((ZRUNTIME_INDEX.deref() as *const HashMap<ZRuntime, AtomicUsize>).read());
    }
}

pub struct ZRuntimePool(HashMap<ZRuntime, OnceLock<Runtime>>);

impl ZRuntimePool {
    fn new() -> Self {
        // Register a callback to clean the static variables.
        unsafe {
            libc::atexit(cleanup);
        }
        Self(ZRuntime::iter().map(|zrt| (zrt, OnceLock::new())).collect())
    }

    pub fn get(&self, zrt: &ZRuntime) -> &Handle {
        // Although the ZRuntime is called to use `zrt`, it may be handover to another one
        // specified via the environmental variable.
        let zrt = match zrt.param().handover {
            Some(handover) => handover,
            None => *zrt,
        };

        self.0
            .get(&zrt)
            .expect("The hashmap should contains {zrt} after initialization")
            .get_or_init(|| zrt.init().expect("Failed to init {zrt}"))
            .handle()
    }
}

// If there are any blocking tasks spawned by ZRuntimes, the function will block until they return.
impl Drop for ZRuntimePool {
    fn drop(&mut self) {
        let handles: Vec<_> = self
            .0
            .drain()
            .filter_map(|(_name, mut rt)| {
                rt.take()
                    .map(|r| std::thread::spawn(move || r.shutdown_timeout(Duration::from_secs(1))))
            })
            .collect();

        for hd in handles {
            let _ = hd.join();
        }
    }
}

#[should_panic(expected = "Zenoh runtime doesn't support")]
#[tokio::test]
async fn block_in_place_fail_test() {
    use crate::ZRuntime;
    ZRuntime::TX.block_in_place(async { println!("Done") });
}
