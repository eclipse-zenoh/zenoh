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
use clap::App;
use async_std::task;
use zenoh::net::*;

fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net write example")
            .arg("-l, --locator=[LOCATOR] 'Sets the locator used to initiate the zenoh session'")
            .arg("-p, --path=[PATH]       'Sets the name of the resource to write'")
            .arg("-v, --value=[VALUE]     'Sets the value of the resource to write'")
            .get_matches();

        let locator = args.value_of("locator").unwrap_or("").to_string();
        let path    = args.value_of("path").unwrap_or("/demo/example/zenoh-rs-write").to_string();
        let value   = args.value_of("value").unwrap_or("Write from Rust!").to_string();

        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        println!("Writing Data ('{}': '{}')...\n", path, value);
        session.write(&path.into(), value.as_bytes().into()).await.unwrap();

        session.close().await.unwrap();
    })
}
