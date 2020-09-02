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
use async_std::sync::channel;
use futures::prelude::*;
use log::{debug, trace};
use zenoh_protocol::core::WhatAmI;
use zenoh_router::runtime::orchestrator::{Loop, SessionOrchestrator};

mod types;
pub use types::*;

mod consts;
pub use consts::*;

#[macro_use]
mod session;
pub use session::*;

pub use zenoh_protocol::proto::{data_kind, encoding};

pub mod queryable {
    pub use zenoh_protocol::core::queryable::*;
}
pub mod utils {
    pub mod resource_name {
        pub use zenoh_protocol::core::rname::intersect;
    }
}

/// Scout for routers and/or peers.
///
/// [scout](scout) spawns a task that periodically sends scout messages and returns
/// a [HelloStream](HelloStream) : a stream of received [Hello](Hello) messages.
///
/// Drop the returned [HelloStream](HelloStream) to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `iface` - The network interface to use for multicast (or "auto")
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::net::*;
/// use futures::prelude::*;
///
/// let mut stream = scout(whatami::PEER | whatami::ROUTER, "auto").await;
/// while let Some(hello) = stream.next().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
pub async fn scout(what: WhatAmI, iface: &str) -> HelloStream {
    debug!("scout({}, {})", what, iface);
    let (hello_sender, hello_receiver) = channel::<Hello>(1);
    let (stop_sender, mut stop_receiver) = channel::<()>(1);
    let iface = SessionOrchestrator::get_interface(iface).unwrap();
    let socket = SessionOrchestrator::bind_ucast_port(iface).await.unwrap();
    async_std::task::spawn(async move {
        let hello_sender = &hello_sender;
        let scout = SessionOrchestrator::scout(&socket, what, async move |hello| {
            hello_sender.send(hello).await;
            Loop::Continue
        });
        let stop = async move {
            stop_receiver.next().await;
            trace!("stop scout({}, {})", what, iface);
        };
        async_std::prelude::FutureExt::race(scout, stop).await;
    });

    HelloStream {
        hello_receiver,
        stop_sender,
    }
}

/// Open a zenoh-net [Session](Session).
///
/// # Arguments
///
/// * `config` - The configuration of the zenoh-net session
/// * `ps` - Optional properties
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::net::*;
///
/// let session = open(Config::peer(), None).await.unwrap();
/// # })
/// ```
pub async fn open(config: Config, ps: Option<Properties>) -> ZResult<Session> {
    debug!("open(\"{}\", {:?})", config, ps);
    Session::new(config, ps).await
}
