//
// Copyright (c) 2025 ZettaScale Technology
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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use arc_swap::{ArcSwap, Guard};

pub struct CacheValue<T: Sized> {
    version: usize,
    value: T,
}

impl<T> CacheValue<T> {
    pub fn get_ref(&self) -> &T {
        &self.value
    }
}

/// This is a lock-free concurrent cache.
/// It stores only the most up-to-date value.
pub struct Cache<T> {
    value: ArcSwap<CacheValue<T>>,
    is_updating: AtomicBool,
}

pub type CacheValueType<T> = Guard<Arc<CacheValue<T>>>;

impl<T> Cache<T> {
    pub fn new(value: T, version: usize) -> Self {
        Cache {
            value: ArcSwap::new(CacheValue::<T> { version, value }.into()),
            is_updating: AtomicBool::new(false),
        }
    }

    fn finish_update(&self) {
        self.is_updating.store(false, Ordering::SeqCst);
    }

    /// Tries to retrive value for the specified version.
    /// If requested version corresponds to the value currently stored in cache - the value is returned.
    /// If requested version is older None will be returned.
    /// If requested version is newer, the new value will be computed and stored by calling f, and then returned.
    pub fn value(&self, version: usize, f: impl Fn() -> T) -> Option<CacheValueType<T>> {
        let v = self.value.load();
        match v.version.cmp(&version) {
            std::cmp::Ordering::Equal => Some(v),
            std::cmp::Ordering::Greater => None, //requesting too old version
            std::cmp::Ordering::Less => {
                // try to update
                drop(v);
                match self.is_updating.compare_exchange(
                    false,
                    true,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        let v = self.value.load();
                        match v.version.cmp(&version) {
                            std::cmp::Ordering::Equal => {
                                // already updated by someone else to the version we need
                                self.finish_update();
                                Some(v)
                            }
                            std::cmp::Ordering::Greater => {
                                // already updated by someone else beyond the version we need
                                self.finish_update();
                                None
                            }
                            std::cmp::Ordering::Less => {
                                self.value.store(
                                    CacheValue {
                                        value: f(),
                                        version,
                                    }
                                    .into(),
                                );
                                self.finish_update();
                                self.value(version, f)
                            }
                        }
                    }
                    Err(_) => None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;

    #[test]
    fn test_cache_single_thread() {
        let cache = Cache::<String>::new("0".to_string(), 0);

        assert_eq!(
            cache
                .value(0, || { "1".to_string() })
                .unwrap()
                .get_ref()
                .as_str(),
            "0"
        );
        assert_eq!(
            cache
                .value(1, || { "1".to_string() })
                .unwrap()
                .get_ref()
                .as_str(),
            "1"
        );
        assert!(cache.value(0, || { "2".to_string() }).is_none());
    }

    //TODO: add multi-threaded tests.
}
