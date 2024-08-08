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
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use zenoh_result::{bail, ZResult};
use zenoh_shm::watchdog::{
    confirmator::GLOBAL_CONFIRMATOR, storage::GLOBAL_STORAGE, validator::GLOBAL_VALIDATOR,
};

pub mod common;
use common::{execute_concurrent, CpuLoad};

const VALIDATION_PERIOD: Duration = Duration::from_millis(100);
const CONFIRMATION_PERIOD: Duration = Duration::from_millis(50);

fn watchdog_alloc_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let _allocated = GLOBAL_STORAGE.read().allocate_watchdog()?;
        Ok(())
    }
}

#[test]
fn watchdog_alloc() {
    execute_concurrent(1, 10000, watchdog_alloc_fn());
}

#[test]
fn watchdog_alloc_concurrent() {
    execute_concurrent(1000, 10000, watchdog_alloc_fn());
}

fn watchdog_confirmed_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated = GLOBAL_STORAGE.read().allocate_watchdog()?;
        let confirmed = GLOBAL_CONFIRMATOR.read().add_owned(&allocated.descriptor)?;

        // check that the confirmed watchdog stays valid
        for i in 0..10 {
            std::thread::sleep(VALIDATION_PERIOD);
            let valid = confirmed.owned.test_validate() != 0;
            if !valid {
                bail!("Invalid watchdog, iteration {i}");
            }
        }
        Ok(())
    }
}

#[test]
#[ignore]
fn watchdog_confirmed() {
    execute_concurrent(1, 10, watchdog_confirmed_fn());
}

#[test]
#[ignore]
fn watchdog_confirmed_concurrent() {
    execute_concurrent(1000, 10, watchdog_confirmed_fn());
}

// TODO: confirmation to dangling watchdog actually writes to potentially-existing
// other watchdog instance from other test running in the same process and changes it's behaviour,
// so we cannot run dangling test in parallel with anything else
#[test]
#[ignore]
fn watchdog_confirmed_dangling() {
    let allocated = GLOBAL_STORAGE
        .read()
        .allocate_watchdog()
        .expect("error allocating watchdog!");
    let confirmed = GLOBAL_CONFIRMATOR
        .read()
        .add_owned(&allocated.descriptor)
        .expect("error adding watchdog to confirmator!");
    drop(allocated);

    // confirm dangling (not allocated) watchdog
    for _ in 0..10 {
        std::thread::sleep(VALIDATION_PERIOD);
        confirmed.owned.confirm();
    }
}

fn watchdog_validated_fn() -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated = GLOBAL_STORAGE.read().allocate_watchdog()?;
        let confirmed = GLOBAL_CONFIRMATOR.read().add_owned(&allocated.descriptor)?;

        let valid = Arc::new(AtomicBool::new(true));
        {
            let c_valid = valid.clone();
            GLOBAL_VALIDATOR.read().add(
                allocated.descriptor.clone(),
                Box::new(move || {
                    c_valid.store(false, std::sync::atomic::Ordering::SeqCst);
                }),
            );
        }

        // check that the watchdog stays valid as it is confirmed
        for i in 0..10 {
            std::thread::sleep(VALIDATION_PERIOD);
            if !valid.load(std::sync::atomic::Ordering::SeqCst) {
                bail!("Invalid watchdog, iteration {i}");
            }
        }

        // Worst-case timings:
        // validation:       |___________|___________|___________|___________|
        // confirmation:    __|_____|_____|_____|_____|
        // drop(confirmed):                            ^
        // It means that the worst-case latency for the watchdog to become invalid is VALIDATION_PERIOD*2

        // check that the watchdog becomes invalid once we stop it's confirmation
        drop(confirmed);
        std::thread::sleep(VALIDATION_PERIOD * 3 + CONFIRMATION_PERIOD);
        assert!(!valid.load(std::sync::atomic::Ordering::SeqCst));

        Ok(())
    }
}

#[test]
#[ignore]
fn watchdog_validated() {
    execute_concurrent(1, 10, watchdog_validated_fn());
}

#[test]
#[ignore]
fn watchdog_validated_concurrent() {
    execute_concurrent(1000, 10, watchdog_validated_fn());
}

fn watchdog_validated_invalid_without_confirmator_fn(
) -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated = GLOBAL_STORAGE
            .read()
            .allocate_watchdog()
            .expect("error allocating watchdog!");

        let valid = Arc::new(AtomicBool::new(true));
        {
            let c_valid = valid.clone();
            GLOBAL_VALIDATOR.read().add(
                allocated.descriptor.clone(),
                Box::new(move || {
                    c_valid.store(false, std::sync::atomic::Ordering::SeqCst);
                }),
            );
        }

        assert!(allocated.descriptor.test_validate() == 0);

        // check that the watchdog becomes invalid because we do not confirm it
        std::thread::sleep(VALIDATION_PERIOD * 2 + CONFIRMATION_PERIOD);
        assert!(!valid.load(std::sync::atomic::Ordering::SeqCst));
        Ok(())
    }
}

#[test]
#[ignore]
fn watchdog_validated_invalid_without_confirmator() {
    execute_concurrent(1, 10, watchdog_validated_invalid_without_confirmator_fn());
}

#[test]
#[ignore]
fn watchdog_validated_invalid_without_confirmator_concurrent() {
    execute_concurrent(
        1000,
        10,
        watchdog_validated_invalid_without_confirmator_fn(),
    );
}

fn watchdog_validated_additional_confirmation_fn(
) -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated = GLOBAL_STORAGE
            .read()
            .allocate_watchdog()
            .expect("error allocating watchdog!");
        let confirmed = GLOBAL_CONFIRMATOR
            .read()
            .add_owned(&allocated.descriptor)
            .expect("error adding watchdog to confirmator!");

        let allow_invalid = Arc::new(AtomicBool::new(false));
        {
            let c_allow_invalid = allow_invalid.clone();
            GLOBAL_VALIDATOR.read().add(
                allocated.descriptor.clone(),
                Box::new(move || {
                    assert!(c_allow_invalid.load(std::sync::atomic::Ordering::SeqCst));
                    c_allow_invalid.store(false, std::sync::atomic::Ordering::SeqCst);
                }),
            );
        }

        // make additional confirmations
        for _ in 0..100 {
            std::thread::sleep(VALIDATION_PERIOD / 10);
            confirmed.owned.confirm();
        }

        // check that the watchdog stays valid as we stop additional confirmation
        std::thread::sleep(VALIDATION_PERIOD * 10);

        // Worst-case timings:
        // validation:       |___________|___________|___________|___________|
        // confirmation:    __|_____|_____|_____|_____|
        // drop(confirmed):                            ^
        // It means that the worst-case latency for the watchdog to become invalid is VALIDATION_PERIOD*2

        // check that the watchdog becomes invalid once we stop it's regular confirmation
        drop(confirmed);
        allow_invalid.store(true, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(VALIDATION_PERIOD * 2 + CONFIRMATION_PERIOD);
        // check that invalidation event happened!
        assert!(!allow_invalid.load(std::sync::atomic::Ordering::SeqCst));
        Ok(())
    }
}

#[test]
#[ignore]
fn watchdog_validated_additional_confirmation() {
    execute_concurrent(1, 10, watchdog_validated_additional_confirmation_fn());
}

#[test]
#[ignore]
fn watchdog_validated_additional_confirmation_concurrent() {
    execute_concurrent(1000, 10, watchdog_validated_additional_confirmation_fn());
}

fn watchdog_validated_overloaded_system_fn(
) -> impl Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static {
    |_task_index: usize, _iteration: usize| -> ZResult<()> {
        let allocated = GLOBAL_STORAGE
            .read()
            .allocate_watchdog()
            .expect("error allocating watchdog!");
        let confirmed = GLOBAL_CONFIRMATOR
            .read()
            .add_owned(&allocated.descriptor)
            .expect("error adding watchdog to confirmator!");

        let allow_invalid = Arc::new(AtomicBool::new(false));
        {
            let c_allow_invalid = allow_invalid.clone();
            GLOBAL_VALIDATOR.read().add(
                allocated.descriptor.clone(),
                Box::new(move || {
                    assert!(c_allow_invalid.load(std::sync::atomic::Ordering::SeqCst));
                    c_allow_invalid.store(false, std::sync::atomic::Ordering::SeqCst);
                }),
            );
        }

        // check that the watchdog stays valid
        std::thread::sleep(VALIDATION_PERIOD * 10);

        // Worst-case timings:
        // validation:       |___________|___________|___________|___________|
        // confirmation:    __|_____|_____|_____|_____|
        // drop(confirmed):                            ^
        // It means that the worst-case latency for the watchdog to become invalid is VALIDATION_PERIOD*2

        // check that the watchdog becomes invalid once we stop it's regular confirmation
        drop(confirmed);
        allow_invalid.store(true, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(VALIDATION_PERIOD * 2 + CONFIRMATION_PERIOD);
        // check that invalidation event happened!
        assert!(!allow_invalid.load(std::sync::atomic::Ordering::SeqCst));
        Ok(())
    }
}

#[test]
#[ignore]
fn watchdog_validated_low_load() {
    let _load = CpuLoad::low();
    execute_concurrent(1000, 10, watchdog_validated_overloaded_system_fn());
}

#[test]
#[ignore]
fn watchdog_validated_high_load() {
    let _load = CpuLoad::optimal_high();
    execute_concurrent(1000, 10, watchdog_validated_overloaded_system_fn());
}

#[test]
#[ignore]
fn watchdog_validated_overloaded_system() {
    let _load = CpuLoad::excessive();
    execute_concurrent(1000, 10, watchdog_validated_overloaded_system_fn());
}
