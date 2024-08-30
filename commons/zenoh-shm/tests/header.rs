//
// Copyright (c) 2023 ZettaScale Technology
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
#![cfg(feature = "test")]
use std::sync::atomic::Ordering::Relaxed;

use rand::Rng;
use zenoh_result::ZResult;
use zenoh_shm::header::{
    descriptor::HeaderDescriptor, storage::GLOBAL_HEADER_STORAGE,
    subscription::GLOBAL_HEADER_SUBSCRIPTION,
};

pub mod common;
use common::execute_concurrent;

fn header_alloc_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let _allocated_header = GLOBAL_HEADER_STORAGE.read().allocate_header()?;
        Ok(())
    }
}

#[test]
fn header_alloc() {
    execute_concurrent(1, 1000, header_alloc_fn());
}

#[test]
fn header_alloc_concurrent() {
    execute_concurrent(100, 1000, header_alloc_fn());
}

fn header_link_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| {
        let allocated_header = GLOBAL_HEADER_STORAGE.read().allocate_header()?;
        let descr = HeaderDescriptor::from(&allocated_header.descriptor);
        let _linked_header = GLOBAL_HEADER_SUBSCRIPTION.read().link(&descr)?;
        Ok(())
    }
}

#[test]
fn header_link() {
    execute_concurrent(1, 1000, header_link_fn());
}

#[test]
fn header_link_concurrent() {
    execute_concurrent(100, 1000, header_link_fn());
}

fn header_link_failure_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static
{
    |_task_index: usize, _iteration: usize| {
        let allocated_header = GLOBAL_HEADER_STORAGE.read().allocate_header()?;
        let descr = HeaderDescriptor::from(&allocated_header.descriptor);
        drop(allocated_header);

        // Some comments on this behaviour...
        // Even though the allocated_header is dropped, it's SHM segment still exists in GLOBAL_HEADER_STORAGE,
        // so there is no way to detect that header is "deallocated" and the code below succeeds. The invalidation
        // functionality is implemented on higher level by means of generation mechanism and protects from both header
        // and watchdog link-to-deallocated issues. This generation mechanism depends on the behaviour below, so
        // everything is fair :)
        let _linked_header = GLOBAL_HEADER_SUBSCRIPTION.read().link(&descr)?;
        Ok(())
    }
}

#[test]
fn header_link_failure() {
    execute_concurrent(1, 1000, header_link_failure_fn());
}

#[test]
fn header_link_failure_concurrent() {
    execute_concurrent(100, 1000, header_link_failure_fn());
}

fn header_check_memory_fn(parallel_tasks: usize, iterations: usize) {
    let task_fun = |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated_header = GLOBAL_HEADER_STORAGE.read().allocate_header()?;
        let descr = HeaderDescriptor::from(&allocated_header.descriptor);
        let linked_header = GLOBAL_HEADER_SUBSCRIPTION.read().link(&descr)?;

        let mut rng = rand::thread_rng();
        let allocated = allocated_header.descriptor.header();
        let linked = linked_header.header();
        for _ in 0..100 {
            let gen = rng.gen();
            allocated.generation.store(gen, Relaxed);
            assert_eq!(gen, linked.generation.load(Relaxed));

            let rc = rng.gen();
            allocated.refcount.store(rc, Relaxed);
            assert_eq!(rc, linked.refcount.load(Relaxed));

            let watchdog_inv = rng.gen();
            allocated.watchdog_invalidated.store(watchdog_inv, Relaxed);
            assert_eq!(watchdog_inv, linked.watchdog_invalidated.load(Relaxed));

            assert_eq!(gen, linked.generation.load(Relaxed));
            assert_eq!(rc, linked.refcount.load(Relaxed));
            assert_eq!(watchdog_inv, linked.watchdog_invalidated.load(Relaxed));
        }
        Ok(())
    };
    execute_concurrent(parallel_tasks, iterations, task_fun);
}

#[test]
fn header_check_memory() {
    header_check_memory_fn(1, 1000);
}

#[test]
fn header_check_memory_concurrent() {
    header_check_memory_fn(100, 100);
}
