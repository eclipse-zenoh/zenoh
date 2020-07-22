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

/// The network level zenoh API.
/// 
/// # Examples
/// 
/// Publish
/// ```
/// use zenoh::net::*;
/// 
/// #[async_std::main]
/// async fn main() {
///     let session = open(Config::default(), None).await.unwrap();
///     session.write(&"/resource/name".into(), "value".as_bytes().into()).await.unwrap();
///     session.close().await.unwrap();
/// }
/// ```
/// 
/// Subscribe
/// ```no_run
/// use zenoh::net::*;
/// use futures::prelude::*;
/// 
/// #[async_std::main]
/// async fn main() {
///     let session = open(Config::default(), None).await.unwrap();
///     let sub_info = SubInfo {
///         reliability: Reliability::Reliable,
///         mode: SubMode::Push,
///         period: None
///     };
///     let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
///     while let Some(sample) = subscriber.next().await { println!("Received : {:?}", sample); };
/// }
/// ```
/// 
/// Query
/// ```
/// use zenoh::net::*;
/// use futures::prelude::*;
/// 
/// #[async_std::main]
/// async fn main() {
///     let session = open(Config::default(), None).await.unwrap();
///     let mut replies = session.query(
///         &"/resource/name".into(),
///         "predicate",
///         QueryTarget::default(),
///         QueryConsolidation::default()
///     ).await.unwrap();
///     while let Some(reply) = replies.next().await {
///         println!(">> Received {:?}", reply.data);
///     }
/// }
/// ```
pub mod net;
use net::{Session, ZResult};

mod workspace;
pub use workspace::{Data, Workspace};

mod properties;
pub use properties::Properties;
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
        let zn_props = props.map(|p| p.0.iter().filter_map(prop_to_zn_prop).collect());
        Ok(Zenoh { session: net::open(config, zn_props).await? })
    }

    pub async fn workspace(&self, prefix: Option<Path>) -> ZResult<Workspace> {
        debug!("New workspace with prefix: {:?}", prefix);
        Workspace::new(self.session.clone(), prefix).await
    }

    pub async fn close(&self) -> ZResult<()> {
        self.session.close().await
    }


}

fn prop_to_zn_prop(_prop: (&String, &String)) -> Option<(net::ZInt, Vec<u8>)> {
    // No existing zenoh-net properties to be mapped yet
    None
}
