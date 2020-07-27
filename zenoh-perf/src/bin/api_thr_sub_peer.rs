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
use async_std::future;
use async_std::sync::Arc;
use async_std::task;
use clap::{App, Arg};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use zenoh::net::ResKey::*;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = App::new("zenoh-net throughput sub example")
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .get_matches();

    let config = Config::new("peer").unwrap().add_peers(
        args.values_of("peer")
            .map(|p| p.collect())
            .or_else(|| Some(vec![]))
            .unwrap(),
    );

    let session = open(config, None).await.unwrap();

    let reskey = RId(session
        .declare_resource(&RName("/test/thr".to_string()))
        .await
        .unwrap());

    let messages = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicUsize::new(0));

    let c_messages = messages.clone();
    let c_bytes = bytes.clone();
    task::spawn(async move {
        println!("messages,bytes");
        loop {
            task::sleep(Duration::from_secs(1)).await;
            let messages = c_messages.swap(0, Ordering::Relaxed);
            let bytes = c_bytes.swap(0, Ordering::Relaxed);
            println!("{},{}", messages, bytes);
        }
    });

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    session
        .declare_callback_subscriber(
            &reskey,
            &sub_info,
            move |_res_name: &str, payload: RBuf, _data_info: Option<RBuf>| {
                messages.fetch_add(1, Ordering::Relaxed);
                bytes.fetch_add(payload.len(), Ordering::Relaxed);
            },
        )
        .await
        .unwrap();

    // Stop forever
    future::pending::<()>().await;

    // @TODO: Uncomment these once the writer starvation has been solved on the RwLock
    // session.undeclare_subscriber(sub).await.unwrap();
    // session.close().await.unwrap();
}
