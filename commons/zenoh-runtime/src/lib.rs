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
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    env,
    future::Future,
    ops::Deref,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::Duration,
};
use tokio::runtime::{Handle, Runtime};
use zenoh_collections::Properties;
use zenoh_result::ZResult as Result;

const ZENOH_RUNTIME_THREADS_ENV: &str = "ZENOH_RUNTIME_THREADS";

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub enum ZRuntime {
    Application,
    Acceptor,
    TX,
    RX,
    Net,
}

impl ZRuntime {
    fn iter() -> impl Iterator<Item = ZRuntime> {
        use ZRuntime::*;
        [Application, Acceptor, TX, RX, Net].into_iter()
    }

    fn init(&self) -> Result<Runtime> {
        // dbg!(*ZRUNTIME_CONFIG);
        let config = &ZRUNTIME_CONFIG;

        let thread_name = format!("{self:?}");

        use ZRuntime::*;
        let rt = match self {
            Application => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.application_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
            Acceptor => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.acceptor_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
            TX => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.tx_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
            RX => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.rx_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
            Net => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.net_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
        };

        Ok(rt)
    }

    pub fn block_in_place<F, R>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
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
    pub static ref ZRUNTIME_CONFIG: ZRuntimeConfig = ZRuntimeConfig::from_env();
    pub static ref ZRUNTIME_POOL: ZRuntimePool = ZRuntimePool::new();
}

pub struct ZRuntimePool(HashMap<ZRuntime, OnceLock<Runtime>>);

impl ZRuntimePool {
    fn new() -> Self {
        Self(ZRuntime::iter().map(|zrt| (zrt, OnceLock::new())).collect())
    }

    pub fn get(&self, zrt: &ZRuntime) -> &Handle {
        self.0
            .get(zrt)
            .expect("The hashmap should contains {zrt} after initialization")
            .get_or_init(|| zrt.init().expect("Failed to init {zrt}"))
            .handle()
    }
}

/// Force drops ZRUNTIME_POOL.
///
/// Rust does not drop static variables. They are always reported by valgrind for example.
/// This function can be used to force drop ZRUNTIME_POOL, to prevent valgrind reporting memory leaks related to it.
/// If there are any blocking tasks spawned by ZRuntimes, the function will block until they return.
#[doc(hidden)]
pub unsafe fn zruntime_pool_free() {
    let fut = || {
        let hm = &ZRUNTIME_POOL.0 as *const HashMap<ZRuntime, OnceLock<Runtime>>
            as *mut HashMap<ZRuntime, OnceLock<Runtime>>;
        for (_k, v) in hm.as_mut().unwrap().drain() {
            let r = v.get().unwrap();
            let r_mut = (r as *const Runtime) as *mut Runtime;
            r_mut.read().shutdown_timeout(Duration::from_secs(1));
            std::mem::forget(v);
        }
        std::mem::drop(hm.read());
    };
    // the runtime can only be dropped in the blocking context
    let _ = futures::executor::block_on(tokio::task::spawn_blocking(fut));
}

#[doc(hidden)]
pub struct ZRuntimePoolGuard;

impl Drop for ZRuntimePoolGuard {
    fn drop(&mut self) {
        unsafe {
            zruntime_pool_free();
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ZRuntimeConfig {
    pub application_threads: usize,
    pub acceptor_threads: usize,
    pub tx_threads: usize,
    pub rx_threads: usize,
    pub net_threads: usize,
}

impl ZRuntimeConfig {
    fn from_env() -> ZRuntimeConfig {
        let mut c = Self::default();

        if let Ok(s) = env::var(ZENOH_RUNTIME_THREADS_ENV) {
            let ps = Properties::from(s);
            if let Some(n) = ps.get("tx") {
                if let Ok(n) = n.parse::<usize>() {
                    c.tx_threads = n;
                }
            }
            if let Some(n) = ps.get("rx") {
                if let Ok(n) = n.parse::<usize>() {
                    c.rx_threads = n;
                }
            }
            if let Some(n) = ps.get("net") {
                if let Ok(n) = n.parse::<usize>() {
                    c.net_threads = n;
                }
            }
            if let Some(n) = ps.get("acceptor") {
                if let Ok(n) = n.parse::<usize>() {
                    c.acceptor_threads = n;
                }
            }
            if let Some(n) = ps.get("application") {
                if let Ok(n) = n.parse::<usize>() {
                    c.application_threads = n;
                }
            }
        }

        c
    }
}

// WARN: at least two otherwise fail on the routing test
impl Default for ZRuntimeConfig {
    fn default() -> Self {
        Self {
            application_threads: 2,
            acceptor_threads: 2,
            tx_threads: 2,
            rx_threads: 2,
            net_threads: 2,
        }
    }
}
