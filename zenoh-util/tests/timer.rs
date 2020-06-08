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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use zenoh_util::collections::{Timer, Timed, TimedEvent};

#[derive(Clone)]
struct MyEvent {
    counter: Arc<AtomicUsize>
}

#[async_trait]
impl Timed for MyEvent {
    async fn run(&mut self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

async fn run() {
    // Create the timer
    let mut timer = Timer::new();

    // Counter for testing
    let counter = Arc::new(AtomicUsize::new(0));

    // Create my custom event
    let myev = MyEvent {
        counter: counter.clone()
    };

    // Default testing interval: 50 ms
    let interval = Duration::from_millis(50);

    /* [1] */
    // Fire a once timed event
    let now = Instant::now(); 
    let event = TimedEvent::once(now + (2 * interval), myev.clone());

    // Add the event to the timer
    timer.add(event).await;

    // Wait for the event to occur
    task::sleep(3 * interval).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, 1);

    /* [2] */
    // Fire a once timed event and defuse it before it is executed
    let now = Instant::now(); 
    let event = TimedEvent::once(now + (2 * interval), myev.clone());
    let handle = event.get_handle();

    // Add the event to the timer
    timer.add(event).await;
    //
    handle.defuse();

    // Wait for the event to occur
    task::sleep(3 * interval).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, 0);

    /* [3] */
    // Number of events to occur
    let amount: usize = 5;

    // Half the waiting interval for granularity reasons
    let to_elapse = (2 * amount as u32) * interval;

    // Fire a periodic event
    let event = TimedEvent::periodic(2 * interval, myev.clone());
    let handle = event.get_handle();

    // Add the event to the timer
    timer.add(event).await;

    // Wait for the events to occur
    task::sleep(to_elapse + interval).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, amount);

    // Defuse the event
    handle.defuse();

    // Wait a bit more to verify that not more events have been fired
    task::sleep(to_elapse).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, 0);

    /* [4] */
    // Fire a periodic event
    let event = TimedEvent::periodic(2 * interval, myev);

    // Add the event to the timer
    timer.add(event).await;

    // Wait for the events to occur
    task::sleep(to_elapse + interval).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, amount);

    // Stop the timer
    timer.stop().await;

    // Wait some time
    task::sleep(to_elapse).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, 0);

    // Restart the timer
    timer.start().await;

    // Wait for the events to occur
    task::sleep(to_elapse).await;

    // Load and reset the counter value
    let value = counter.swap(0, Ordering::SeqCst);
    assert_eq!(value, amount);
}
#[test]
fn timer() {
    task::block_on(run());
}