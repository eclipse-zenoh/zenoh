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
use async_std::task;
use zenoh::net::*;

fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();
    
        println!("Scouting...");
        let locs = scout("auto", 10, 500000).await;
        if !locs.is_empty() {
            for l in locs {
                println!("Locator: {}", l);
            }
        } else {
            println!("Did not find any zenoh router.");
        }
    })
}
