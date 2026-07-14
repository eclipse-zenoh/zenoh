//
// Copyright (c) 2026 ZettaScale Technology
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

use std::{ops::Deref, sync::Arc};

use crate::{
    common::shm::handoff::PriorityContainer,
    unicast::establishment::ext::shm::{
        auth::AuthUnicast,
        segment::{RXAuthSegment, ShmCounterID, ShmRXCounterLease, ShmTXCounterLease},
    },
};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::core::{Priority, Reliability};
use zenoh_result::ZResult;

#[derive(Debug)]
pub enum HandoffConfig<T: Sized> {
    Disabled,
    SinglePrio(T),
    PerPrio(Box<PriorityContainer<T>>),
}

impl<T: Sized> HandoffConfig<T> {
    pub fn from_fn(
        multiprio: bool,
        reliability: Reliability,
        ctor_fn: impl Fn(Priority) -> ZResult<T>,
    ) -> ZResult<Self> {
        Ok(match (multiprio, reliability) {
            (_, Reliability::BestEffort) => Self::Disabled,
            (true, Reliability::Reliable) => {
                Self::PerPrio(Box::new(PriorityContainer::from_fn(ctor_fn)?))
            }
            (false, Reliability::Reliable) => Self::SinglePrio(ctor_fn(Priority::default())?),
        })
    }
}

pub type RxHandoffChannel = HandoffConfig<ShmRXCounterLease>;

impl RxHandoffChannel {
    pub fn new_rx(segment: &Arc<RXAuthSegment>, ids: HandoffCounterIds) -> Self {
        match ids {
            HandoffCounterIds::Disabled => Self::Disabled,
            HandoffCounterIds::SinglePrio(counter_id) => {
                let rx_counter = ShmRXCounterLease::new(segment.clone(), counter_id);
                Self::SinglePrio(rx_counter)
            }
            HandoffCounterIds::PerPrio(prio_container) => {
                let prio_container = prio_container
                    .deref()
                    .map(|counter_id| ShmRXCounterLease::new(segment.clone(), *counter_id));
                Self::PerPrio(Box::new(prio_container))
            }
        }
    }

    pub fn on_rx(&self, priority: Priority) {
        match self {
            HandoffConfig::SinglePrio(counter) => counter.counter_decrease(),
            HandoffConfig::PerPrio(prio_container) => {
                prio_container.deref()[priority].counter_decrease()
            }
            HandoffConfig::Disabled => {}
        }
    }
}

#[derive(Debug)]
pub struct TxHandoffChannel(HandoffConfig<ShmTXCounterLease>);
impl TxHandoffChannel {
    pub fn new_disabled() -> Self {
Self(HandoffConfig::Disabled)
    }

    pub fn new_tx(multiprio: bool, reliability: Reliability, auth: &AuthUnicast) -> ZResult<Self> {
        let ctor_fn = |_prio| auth.lease_tx_counter();
        Ok(Self(HandoffConfig::from_fn(
            multiprio,
            reliability,
            ctor_fn,
        )?))
    }

    pub fn on_tx(&self, priority: Priority) {
        match &self.0 {
            HandoffConfig::SinglePrio(counter) => counter.counter_increase(),
            HandoffConfig::PerPrio(prio_container) => {
                prio_container.deref()[priority].counter_increase()
            }
            HandoffConfig::Disabled => {}
        }
    }

    pub fn ids(&self) -> HandoffCounterIds {
        match &self.0 {
            HandoffConfig::SinglePrio(counter) => HandoffCounterIds::SinglePrio(counter.id()),
            HandoffConfig::PerPrio(prio_container) => {
                let prio_container = prio_container.deref();
                let prio_container = prio_container.map(|counter| counter.id());
                HandoffCounterIds::PerPrio(Box::new(prio_container))
            }
            HandoffConfig::Disabled => HandoffCounterIds::Disabled,
        }
    }
}

impl<'a, 'b, W> WCodec<&'a PriorityContainer<ShmCounterID>, &'b mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PriorityContainer<ShmCounterID>) -> Self::Output {
        for obj in x {
            self.write(&mut *writer, obj)?;
        }
        Ok(())
    }
}

impl<'a, R> RCodec<PriorityContainer<ShmCounterID>, &'a mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<PriorityContainer<ShmCounterID>, Self::Error> {
        PriorityContainer::from_fn(|_| self.read(&mut *reader))
    }
}

pub type HandoffCounterIds = HandoffConfig<ShmCounterID>;

impl<W> WCodec<&HandoffCounterIds, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &HandoffCounterIds) -> Self::Output {
        match x {
            HandoffConfig::Disabled => {
                self.write(&mut *writer, &0u8)?;
            }
            HandoffConfig::SinglePrio(counter_id) => {
                self.write(&mut *writer, &1u8)?;
                self.write(&mut *writer, counter_id)?;
            }
            HandoffConfig::PerPrio(prio_container) => {
                self.write(&mut *writer, &2u8)?;
                self.write(&mut *writer, prio_container.deref())?;
            }
        }
        Ok(())
    }
}

impl<R> RCodec<HandoffCounterIds, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<HandoffCounterIds, Self::Error> {
        let multiprio: u8 = self.read(&mut *reader)?;
        if multiprio == 0 {
            let prio: PriorityContainer<ShmCounterID> = self.read(&mut *reader)?;
            Ok(HandoffCounterIds::PerPrio(Box::new(prio)))
        } else {
            let counter_id = self.read(&mut *reader)?;
            Ok(HandoffCounterIds::SinglePrio(counter_id))
        }
    }
}
