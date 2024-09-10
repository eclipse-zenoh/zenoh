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
use zenoh::{config::WhatAmI, scout, Config};

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    println!("Scouting...");
    let receiver = scout(WhatAmI::Peer | WhatAmI::Router, Config::default())
        .await
        .unwrap();

    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while let Ok(hello) = receiver.recv_async().await {
            println!("{hello}");
        }
    })
    .await;

    // stop scouting
    receiver.stop();
}
