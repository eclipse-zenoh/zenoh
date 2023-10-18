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
use async_std::prelude::FutureExt;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh::scouting::WhatAmI;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    println!("Scouting...");
    let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, Config::default())
        .res()
        .await
        .unwrap();

    let _ = async {
        while let Ok(hello) = receiver.recv_async().await {
            println!("{hello}");
        }
    }
    .timeout(std::time::Duration::from_secs(1))
    .await;

    // stop scouting
    drop(receiver);
}
