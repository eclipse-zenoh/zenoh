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
mod batch;
mod defragmentation;
mod events;
mod link;
#[macro_use]
mod main;
// mod reliability_queue;
mod rx;
mod scheduling;
mod tx;

use batch::*;
use defragmentation::*;
use events::*;
use link::*;
// use reliability_queue::*;
use rx::*;
use tx::*;

pub(crate) use main::*;

use crate::proto::ZenohMessage;
use async_std::sync::{Arc, RwLock};
use async_trait::async_trait;

#[async_trait]
trait Scheduling {
    async fn schedule(&self, msg: ZenohMessage, links: &Arc<RwLock<Vec<ChannelLink>>>);
}
