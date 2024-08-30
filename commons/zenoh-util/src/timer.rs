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
use std::{
    cmp::Ordering as ComparisonOrdering,
    collections::BinaryHeap,
    sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        Arc, Weak,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use flume::{bounded, Receiver, RecvError, Sender};
use tokio::{runtime::Handle, select, sync::Mutex, task, time};
use zenoh_core::zconfigurable;

zconfigurable! {
    static ref TIMER_EVENTS_CHANNEL_SIZE: usize = 1;
}

#[async_trait]
pub trait Timed {
    async fn run(&mut self);
}

type TimedFuture = Arc<dyn Timed + Send + Sync>;

#[derive(Clone)]
pub struct TimedHandle(Weak<AtomicBool>);

impl TimedHandle {
    pub fn defuse(self) {
        if let Some(arc) = self.0.upgrade() {
            arc.store(false, AtomicOrdering::Release);
        }
    }
}

#[derive(Clone)]
pub struct TimedEvent {
    when: Instant,
    period: Option<Duration>,
    future: TimedFuture,
    fused: Arc<AtomicBool>,
}

impl TimedEvent {
    pub fn once(when: Instant, event: impl Timed + Send + Sync + 'static) -> TimedEvent {
        TimedEvent {
            when,
            period: None,
            future: Arc::new(event),
            fused: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn periodic(interval: Duration, event: impl Timed + Send + Sync + 'static) -> TimedEvent {
        TimedEvent {
            when: Instant::now() + interval,
            period: Some(interval),
            future: Arc::new(event),
            fused: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_fused(&self) -> bool {
        self.fused.load(AtomicOrdering::Acquire)
    }

    pub fn get_handle(&self) -> TimedHandle {
        TimedHandle(Arc::downgrade(&self.fused))
    }
}

impl Eq for TimedEvent {}

impl Ord for TimedEvent {
    fn cmp(&self, other: &Self) -> ComparisonOrdering {
        // The usual cmp is defined as: self.when.cmp(&other.when)
        // This would make the events ordered from largest to the smallest in the heap.
        // However, we want the events to be ordered from the smallest to the largest.
        // As a consequence of this, we swap the comparison terms, converting the heap
        // from a max-heap into a min-heap.
        other.when.cmp(&self.when)
    }
}

impl PartialOrd for TimedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<ComparisonOrdering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TimedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}

async fn timer_task(
    events: Arc<Mutex<BinaryHeap<TimedEvent>>>,
    new_event: Receiver<(bool, TimedEvent)>,
) -> Result<(), RecvError> {
    // Error message
    let e = "Timer has been dropped. Unable to run timed events.";

    // Acquire the lock
    let mut events = events.lock().await;

    loop {
        // Future for adding new events
        let new = new_event.recv_async();

        match events.peek() {
            Some(next) => {
                // Future for waiting an event timing
                let wait = async {
                    let next = next.clone();
                    let now = Instant::now();
                    if next.when > now {
                        time::sleep(next.when - now).await;
                    }
                    Ok((false, next))
                };

                let result = select! {
                    result = wait => { result },
                    result = new => { result },
                };

                match result {
                    Ok((is_new, mut ev)) => {
                        if is_new {
                            // A new event has just been added: push it onto the heap
                            events.push(ev);
                            continue;
                        }

                        // We are ready to serve the event, remove it from the heap
                        let _ = events.pop();

                        // Execute the future if the event is fused
                        if ev.is_fused() {
                            // Now there is only one Arc pointing to the event future
                            // It is safe to access and execute to the inner future as mutable
                            Arc::get_mut(&mut ev.future).unwrap().run().await;

                            // Check if the event is periodic
                            if let Some(interval) = ev.period {
                                ev.when = Instant::now() + interval;
                                events.push(ev);
                            }
                        }
                    }
                    Err(_) => {
                        // Channel error
                        tracing::trace!("{}", e);
                        return Ok(());
                    }
                }
            }
            None => match new.await {
                Ok((_, ev)) => {
                    events.push(ev);
                    continue;
                }
                Err(_) => {
                    // Channel error
                    tracing::trace!("{}", e);
                    return Ok(());
                }
            },
        }
    }
}

#[derive(Clone)]
pub struct Timer {
    events: Arc<Mutex<BinaryHeap<TimedEvent>>>,
    sl_sender: Option<Sender<()>>,
    ev_sender: Option<Sender<(bool, TimedEvent)>>,
}

impl Timer {
    pub fn new(spawn_blocking: bool) -> Timer {
        // Create the channels
        let (ev_sender, ev_receiver) = bounded::<(bool, TimedEvent)>(*TIMER_EVENTS_CHANNEL_SIZE);
        let (sl_sender, sl_receiver) = bounded::<()>(1);

        // Create the timer object
        let timer = Timer {
            events: Arc::new(Mutex::new(BinaryHeap::new())),
            sl_sender: Some(sl_sender),
            ev_sender: Some(ev_sender),
        };

        // Start the timer task
        let c_e = timer.events.clone();
        let fut = async move {
            select! {
                _ = sl_receiver.recv_async() => {},
                _ = timer_task(c_e, ev_receiver) => {},
            };
            tracing::trace!("A - Timer task no longer running...");
        };
        if spawn_blocking {
            task::spawn_blocking(|| Handle::current().block_on(fut));
        } else {
            task::spawn(fut);
        }

        // Return the timer object
        timer
    }

    pub fn start(&mut self, spawn_blocking: bool) {
        if self.sl_sender.is_none() {
            // Create the channels
            let (ev_sender, ev_receiver) =
                bounded::<(bool, TimedEvent)>(*TIMER_EVENTS_CHANNEL_SIZE);
            let (sl_sender, sl_receiver) = bounded::<()>(1);

            // Store the channels handlers
            self.sl_sender = Some(sl_sender);
            self.ev_sender = Some(ev_sender);

            // Start the timer task
            let c_e = self.events.clone();
            let fut = async move {
                select! {
                    _ = sl_receiver.recv_async() => {},
                    _ = timer_task(c_e, ev_receiver) => {},
                };
                tracing::trace!("A - Timer task no longer running...");
            };
            if spawn_blocking {
                task::spawn_blocking(|| Handle::current().block_on(fut));
            } else {
                task::spawn(fut);
            }
        }
    }

    #[inline]
    pub async fn start_async(&mut self, spawn_blocking: bool) {
        self.start(spawn_blocking)
    }

    pub fn stop(&mut self) {
        if let Some(sl_sender) = &self.sl_sender {
            // Stop the timer task
            let _ = sl_sender.send(());

            tracing::trace!("Stopping timer...");
            // Remove the channels handlers
            self.sl_sender = None;
            self.ev_sender = None;
        }
    }

    pub async fn stop_async(&mut self) {
        if let Some(sl_sender) = &self.sl_sender {
            // Stop the timer task
            let _ = sl_sender.send_async(()).await;

            tracing::trace!("Stopping timer...");
            // Remove the channels handlers
            self.sl_sender = None;
            self.ev_sender = None;
        }
    }

    pub fn add(&self, event: TimedEvent) {
        if let Some(ev_sender) = &self.ev_sender {
            let _ = ev_sender.send((true, event));
        }
    }

    pub async fn add_async(&self, event: TimedEvent) {
        if let Some(ev_sender) = &self.ev_sender {
            let _ = ev_sender.send_async((true, event)).await;
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new(false)
    }
}

mod tests {
    #[test]
    fn timer() {
        use std::{
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
            time::{Duration, Instant},
        };

        use async_trait::async_trait;
        use tokio::{runtime::Runtime, time};

        use super::{Timed, TimedEvent, Timer};

        #[derive(Clone)]
        struct MyEvent {
            counter: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl Timed for MyEvent {
            async fn run(&mut self) {
                self.counter.fetch_add(1, Ordering::SeqCst);
            }
        }

        async fn run() {
            // Create the timer
            let mut timer = Timer::new(false);

            // Counter for testing
            let counter = Arc::new(AtomicUsize::new(0));

            // Create my custom event
            let myev = MyEvent {
                counter: counter.clone(),
            };

            // Default testing interval: 1 s
            let interval = Duration::from_secs(1);

            /* [1] */
            println!("Timer [1]: Once event and run");
            // Fire a once timed event
            let now = Instant::now();
            let event = TimedEvent::once(now + (2 * interval), myev.clone());

            // Add the event to the timer
            timer.add_async(event).await;

            // Wait for the event to occur
            time::sleep(3 * interval).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, 1);

            /* [2] */
            println!("Timer [2]: Once event and defuse");
            // Fire a once timed event and defuse it before it is executed
            let now = Instant::now();
            let event = TimedEvent::once(now + (2 * interval), myev.clone());
            let handle = event.get_handle();

            // Add the event to the timer
            timer.add_async(event).await;
            //
            handle.defuse();

            // Wait for the event to occur
            time::sleep(3 * interval).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, 0);

            /* [3] */
            println!("Timer [3]: Periodic event run and defuse");
            // Number of events to occur
            let amount: usize = 3;

            // Half the waiting interval for granularity reasons
            let to_elapse = (2 * amount as u32) * interval;

            // Fire a periodic event
            let event = TimedEvent::periodic(2 * interval, myev.clone());
            let handle = event.get_handle();

            // Add the event to the timer
            timer.add_async(event).await;

            // Wait for the events to occur
            time::sleep(to_elapse + interval).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, amount);

            // Defuse the event (check if twice defusing don't cause troubles)
            handle.clone().defuse();
            handle.defuse();

            // Wait a bit more to verify that not more events have been fired
            time::sleep(to_elapse).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, 0);

            /* [4] */
            println!("Timer [4]: Periodic event and stop/start timer");
            // Fire a periodic event
            let event = TimedEvent::periodic(2 * interval, myev);

            // Add the event to the timer
            timer.add_async(event).await;

            // Wait for the events to occur
            time::sleep(to_elapse + interval).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, amount);

            // Stop the timer
            timer.stop_async().await;

            // Wait some time
            time::sleep(to_elapse).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, 0);

            // Restart the timer
            timer.start_async(false).await;

            // Wait for the events to occur
            time::sleep(to_elapse).await;

            // Load and reset the counter value
            let value = counter.swap(0, Ordering::SeqCst);
            assert_eq!(value, amount);
        }

        let rt = Runtime::new().unwrap();
        rt.block_on(run());
    }
}
