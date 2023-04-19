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
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_ext::group::*;

#[async_std::main]
async fn main() {
    env_logger::init();
    let z = Arc::new(zenoh::open(Config::default()).res().await.unwrap());
    let member = Member::new(z.zid().to_string())
        .unwrap()
        .lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), "zgroup", member).await.unwrap();
    let rx = group.subscribe().await;
    let mut stream = rx.stream();
    while let Some(evt) = stream.next().await {
        println!(">>> {:?}", &evt);
        println!(">> Group View <<");
        let v = group.view().await;
        println!(
            "{}",
            v.iter()
                .fold(String::from("\n"), |a, b| format!("\t{a} \n\t{b:?}")),
        );
        println!(">>>>>>> Eventual Leader <<<<<<<<<");
        let m = group.leader().await;
        println!("Leader mid = {m:?}");
        println!(">>>>>>><<<<<<<<<");
    }
}
