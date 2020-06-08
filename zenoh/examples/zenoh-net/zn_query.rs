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
use std::time::Duration;
use async_std::task;
use zenoh::net::*;


fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net query example")
            .arg("-l, --locator=[LOCATOR]   'Sets the locator used to initiate the zenoh session'")
            .arg("-s, --selector=[SELECTOR] 'Sets the selection of resources to query'")
            .get_matches();

        let locator  = args.value_of("locator").unwrap_or("").to_string();
        let selector = args.value_of("selector").unwrap_or("/demo/example/**").to_string();

        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        let replies_handler = move |reply: &Reply| {
            match reply {
                Reply::ReplyData {reskey, payload, ..} => {println!(">> [Reply handler] received reply data {:?} : {}", 
                                                            reskey, String::from_utf8_lossy(&payload.to_vec()))}
                Reply::SourceFinal {..} => {println!(">> [Reply handler] received source final.")}
                Reply::ReplyFinal => {println!(">> [Reply handler] received reply final.")}
            }
        };

        println!("Sending Query '{}'...", selector);
        let _eval = session.query(
            &selector.into(), "",
            replies_handler,
            QueryTarget::default(),
            QueryConsolidation::default()
        ).await.unwrap();

        task::sleep(Duration::from_secs(1)).await;

        session.close().await.unwrap();
    })
}
