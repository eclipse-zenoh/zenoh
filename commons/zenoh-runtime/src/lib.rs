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
};
use tokio::runtime::{Handle, Runtime};
use zenoh_collections::Properties;
use zenoh_result::ZResult as Result;

const ZENOH_RUNTIME_THREADS_ENV: &str = "ZENOH_RUNTIME_THREADS";

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub enum ZRuntime {
    Application,
    Reception,
    TX,
    RX,
    Net,
}

impl ZRuntime {
    fn iter() -> impl Iterator<Item = ZRuntime> {
        use ZRuntime::*;
        [Application, Reception, TX, RX, Net].into_iter()
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
            Reception => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.reception_threads)
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

#[derive(Debug, Copy, Clone)]
pub struct ZRuntimeConfig {
    pub application_threads: usize,
    pub reception_threads: usize,
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
            if let Some(n) = ps.get("reception") {
                if let Ok(n) = n.parse::<usize>() {
                    c.reception_threads = n;
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
            application_threads: 1,
            reception_threads: 1,
            tx_threads: 1,
            rx_threads: 1,
            net_threads: 1,
        }
    }
}
