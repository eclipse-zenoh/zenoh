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
#![feature(async_closure)]

#[macro_use]
extern crate lazy_static;

use log::debug;
use async_std::task;

pub mod net;
use net::{Properties, Session, ZResult};

mod workspace;
pub use workspace::Workspace;

mod path;
pub use path::Path;
mod pathexpr;
pub use pathexpr::PathExpr;
mod selector;
pub use selector::Selector;
mod values;
pub use values::*;

type Config = net::Config;

pub struct Zenoh {
    session: Session
}

impl Zenoh {

    pub async fn new(config: Config, props: Option<Properties>) -> ZResult<Zenoh> {
        Ok(Zenoh { session: net::open(config, props).await? })
    }

    pub async fn workspace(&self, prefix: Option<Path>) -> ZResult<Workspace> {
        debug!("New workspace with prefix: {:?}", prefix);
        Workspace::new(self.session.clone(), prefix).await
    }

    pub async fn close(&self) -> ZResult<()> {
        self.session.close().await
    }

}
