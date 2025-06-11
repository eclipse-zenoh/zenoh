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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};

pub trait KeyedEvent {
    fn key_expr(&self) -> &keyexpr;
}

pub trait KeyedHandler<Event> {
    fn handle(&self, event: &Event);
}

pub trait KeyedHandlerMut<Event> {
    fn handle_mut(&mut self, event: &Event);
}

pub struct KeyedHandlers<Event> {
    handlers: Vec<(OwnedKeyExpr, Box<dyn KeyedHandler<Event> + Send + Sync>)>,
}

impl<Event> Default for KeyedHandlers<Event> {
    fn default() -> Self {
        Self {
            handlers: Vec::default(),
        }
    }
}

pub struct KeyedHandlersMut<Event> {
    handlers: Vec<(OwnedKeyExpr, Box<dyn KeyedHandlerMut<Event> + Send + Sync>)>,
}

impl<Event> Default for KeyedHandlersMut<Event> {
    fn default() -> Self {
        Self {
            handlers: Vec::default(),
        }
    }
}

impl<Event> KeyedHandlers<Event> {
    pub fn insert<Handler: KeyedHandler<Event> + Send + Sync + 'static>(
        &mut self,
        key_expr: OwnedKeyExpr,
        handler: Handler,
    ) {
        self.handlers.push((key_expr, Box::new(handler)));
    }
}

impl<Event> KeyedHandlersMut<Event> {
    pub fn insert<Handler: KeyedHandlerMut<Event> + Send + Sync + 'static>(
        &mut self,
        key_expr: OwnedKeyExpr,
        handler: Handler,
    ) {
        self.handlers.push((key_expr, Box::new(handler)));
    }
}

impl<Event> KeyedHandler<Event> for KeyedHandlers<Event>
where
    Event: KeyedEvent,
{
    fn handle(&self, event: &Event) {
        for (k, h) in &self.handlers {
            if event.key_expr().intersects(k) {
                h.handle(event);
            }
        }
    }
}

impl<Event> KeyedHandlerMut<Event> for KeyedHandlersMut<Event>
where
    Event: KeyedEvent,
{
    fn handle_mut(&mut self, event: &Event) {
        for (k, h) in &mut self.handlers {
            if event.key_expr().intersects(k) {
                h.handle_mut(event);
            }
        }
    }
}
