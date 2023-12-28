use lazy_static::lazy_static;
use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::runtime::{Handle, Runtime};
use zenoh_result::{zerror, ZResult as Result};

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub enum ZRuntime {
    TX,
    RX,
    Accept,
    Application,
    Net,
}

impl ZRuntime {
    fn iter() -> impl Iterator<Item = ZRuntime> {
        use ZRuntime::*;
        [TX, RX, Accept, Application, Net].into_iter()
    }

    fn init(&self) -> Result<Runtime> {
        let config = ZRUNTIME_CONFIG.lock().map_err(|e| zerror!("{e}"))?;

        let thread_name = format!("{self:?}");

        use ZRuntime::*;
        let rt = match self {
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
            Accept => tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.accept_threads)
                .enable_io()
                .enable_time()
                .thread_name_fn(move || {
                    static ATOMIC_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                    let id = ATOMIC_THREAD_ID.fetch_add(1, Ordering::SeqCst);
                    format!("{thread_name}-{}", id)
                })
                .build()?,
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

    pub fn block_in_place<F, R>(&self, f: F) -> R where F: Future<Output = R> {
        tokio::task::block_in_place(move || self.block_on(f))
    }
}

impl Deref for ZRuntime {
    type Target = Handle;
    fn deref(&self) -> &Self::Target {
        // ZRUNTIME_POOL.get(&Self::TX)
        // use ZRuntime::*;
        // match self {
        //     Application | Accept | RX => {
        //         ZRUNTIME_POOL.get(&TX)
        //     }
        //     _ => {
        //         ZRUNTIME_POOL.get(&self)
        //     }
        // }
        ZRUNTIME_POOL.get(self)
    }
}

lazy_static! {
    pub static ref ZRUNTIME_CONFIG: Mutex<ZRuntimeConfig> = Mutex::new(ZRuntimeConfig::default());
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

pub struct ZRuntimeConfig {
    pub tx_threads: usize,
    pub rx_threads: usize,
    pub accept_threads: usize,
    pub application_threads: usize,
    pub net_threads: usize,
}

impl Default for ZRuntimeConfig {
    fn default() -> Self {
        Self {
            tx_threads: 2,
            rx_threads: 2,
            accept_threads: 2,
            application_threads: 2,
            net_threads: 2,
            // tx_threads: 1,
            // rx_threads: 1,
            // accept_threads: 1,
            // application_threads: 1,
            // net_threads: 1,
        }
    }
}
