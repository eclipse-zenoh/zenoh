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
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex, Receiver, RecvError, Sender, Weak, channel};
use async_std::task;
use async_trait::async_trait;

use std::cmp::Ordering as ComparisonOrdering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use crate::zconfigurable;

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
            arc.store(false, AtomicOrdering::Relaxed);
        }
    }
}

#[derive(Clone)]
pub struct TimedEvent {
    when: Instant,
    period: Option<Duration>,
    future: TimedFuture,
    fused: Arc<AtomicBool>
}

impl TimedEvent {
    pub fn once(
        when: Instant,
        event: impl Timed + Send + Sync + 'static
    ) -> TimedEvent {
        TimedEvent {
            when,
            period: None,
            future: Arc::new(event),
            fused: Arc::new(AtomicBool::new(true))
        }
    }

    pub fn periodic(
        interval: Duration,
        event: impl Timed + Send + Sync + 'static
    ) -> TimedEvent {
        TimedEvent {
            when: Instant::now() + interval,
            period: Some(interval),
            future: Arc::new(event),
            fused: Arc::new(AtomicBool::new(true))
        }
    }

    pub fn is_fused(&self) -> bool {
        self.fused.load(AtomicOrdering::Relaxed)
    }

    pub fn get_handle(&self) -> TimedHandle {
        TimedHandle(Arc::downgrade(&self.fused))
    }
}

impl Eq for TimedEvent {}

impl Ord for TimedEvent {
    fn cmp(&self, other: &Self) -> ComparisonOrdering {
        // The usual cmp is defined as: self.when.cmp(&other.when)
        // This would make the events odered from largets to the smallest in the heap.
        // However, we want the events to be ordered from the smallets to the largest. 
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
    new_event: Receiver<(bool, TimedEvent)>
) -> Result<(), RecvError> {
    // Error message
    let e = "Timer has been dropped. Unable to run timed events.";

    // Acquire the lock
    let mut events = events.lock().await;
    
    loop { 
        // Fuuture for adding new events
        let new = new_event.recv();

        match events.peek() {
            Some(next) => {
                // Future for waiting an event timing
                let wait = async {    
                    let next = next.clone();
                    let now = Instant::now();
                    if next.when > now {              
                        task::sleep(next.when - now).await;
                    }                        
                    Ok((false, next))
                };

                match new.race(wait).await {
                    Ok((is_new, mut ev)) => {
                        if is_new {
                            // A new event has just been added: push it onto the heap
                            events.push(ev);
                            continue
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
                    },
                    Err(_) => {
                        // Channel error
                        log::trace!("{}", e);                       
                        return Ok(())
                    }    
                }                
            },
            None => match new.await {
                Ok((_, ev)) => {
                    events.push(ev);
                    continue
                },
                Err(_) => {
                    // Channel error
                    log::trace!("{}", e);
                    return Ok(())
                }
            } 
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
    pub fn new() -> Timer {
        // Create the channels
        let (ev_sender, ev_receiver) = channel::<(bool, TimedEvent)>(*TIMER_EVENTS_CHANNEL_SIZE);
        let (sl_sender, sl_receiver) = channel::<()>(1);

        // Create the timer object
        let timer = Timer {
            events: Arc::new(Mutex::new(BinaryHeap::new())),
            sl_sender: Some(sl_sender),
            ev_sender: Some(ev_sender),
        };

        // Start the timer task
        let c_e = timer.events.clone();
        task::spawn(async move {
            let _ = sl_receiver.recv().race(timer_task(c_e, ev_receiver)).await;
            log::trace!("A - Timer task no longer running...");
        });

        // Return the timer object
        timer
    }

    pub async fn start(&mut self) {
        if self.sl_sender.is_none() {
            // Create the channels
            let (ev_sender, ev_receiver) = channel::<(bool, TimedEvent)>(*TIMER_EVENTS_CHANNEL_SIZE);
            let (sl_sender, sl_receiver) = channel::<()>(1);

            // Store the channels handlers
            self.sl_sender = Some(sl_sender);
            self.ev_sender = Some(ev_sender);

            // Start the timer task
            let c_e = self.events.clone();
            task::spawn(async move {
                let _ = sl_receiver.recv().race(timer_task(c_e, ev_receiver)).await;
                log::trace!("B - Timer task no longer running...");
            });
        }
    }

    pub async fn stop(&mut self) {
        if let Some(sl_sender) = &self.sl_sender {
            // Stop the timer task
            sl_sender.send(()).await;

            log::trace!("Stopping timer...");
            // Remove the channels handlers
            self.sl_sender = None;
            self.ev_sender = None;
        }
    }

    pub async fn add(&self, event: TimedEvent) {
        if let Some(ev_sender) = &self.ev_sender {
            ev_sender.send((true, event)).await;
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}