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
use async_std::prelude::*;
use async_std::task;
use zenoh::net::*;
use zenoh::net::queryable::EVAL;

fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net eval example")
            .arg("-l, --locator=[LOCATOR] 'Sets the locator used to initiate the zenoh session'")
            .arg("-p, --path=[PATH]       'Sets the name of the resource to evaluate'")
            .arg("-v, --value=[VALUE]     'Sets the value to reply to queries'")
            .get_matches();

        let locator = args.value_of("locator").unwrap_or("").to_string();
        let path    = args.value_of("path").unwrap_or("/demo/example/zenoh-rs-eval").to_string();
        let value   = args.value_of("value").unwrap_or("Eval from Rust!").to_string();

        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        // We want to use path in query_handler closure.
        // But as this closure must take the ownership, we clone path as rname.
        let rname = path.clone();
        let query_handler = move |res_name: &str, predicate: &str, replies_sender: &RepliesSender, query_handle: QueryHandle| {
            println!(">> [Query handler] Handling '{}?{}'", res_name, predicate);
            let result: Vec<(String, RBuf, Option<RBuf>)> = [(rname.clone(), value.as_bytes().into(), None)].to_vec();
            (*replies_sender)(query_handle, result);
        };

        println!("Declaring Queryable on {}", path);
        let queryable = session.declare_queryable(&path.into(), EVAL, query_handler).await.unwrap();

        let mut stdin = async_std::io::stdin();
        let mut input = [0u8];
        while input[0] != 'q' as u8 {
            stdin.read_exact(&mut input).await.unwrap();
        }

        session.undeclare_queryable(queryable).await.unwrap();
        session.close().await.unwrap();
    })
}
