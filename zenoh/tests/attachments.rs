//
// Copyright (c) 2024 ZettaScale Technology
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
#[cfg(feature = "unstable")]
#[test]
fn pubsub() {
    use zenoh::prelude::sync::*;
    use zenoh_config::Config;

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_subscriber("test/attachment")
        .callback(|sample| {
            println!("{}", sample.payload().deserialize::<String>().unwrap());
            for (k, v) in sample.attachment().unwrap() {
                assert!(k.iter().rev().zip(v.as_slice()).all(|(k, v)| k == v))
            }
        })
        .res()
        .unwrap();
    let publisher = zenoh.declare_publisher("test/attachment").res().unwrap();
    for i in 0..10 {
        let mut backer = [(
            [0; std::mem::size_of::<usize>()],
            [0; std::mem::size_of::<usize>()],
        ); 10];
        for (j, backer) in backer.iter_mut().enumerate() {
            *backer = ((i * 10 + j).to_le_bytes(), (i * 10 + j).to_be_bytes())
        }
        zenoh
            .put("test/attachment", "put")
            .attachment(Some(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            ))
            .res()
            .unwrap();
        publisher
            .put("publisher")
            .attachment(Some(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            ))
            .res()
            .unwrap();
    }
}
#[cfg(feature = "unstable")]
#[test]
fn queries() {
    use zenoh::{prelude::sync::*, sample::Attachment, sample::SampleBuilderTrait};
    use zenoh_config::Config;

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_queryable("test/attachment")
        .callback(|query| {
            println!(
                "{}",
                query
                    .value()
                    .map(|q| q.payload.deserialize::<String>().unwrap())
                    .unwrap_or_default()
            );
            let mut attachment = Attachment::new();
            for (k, v) in query.attachment().unwrap() {
                assert!(k.iter().rev().zip(v.as_slice()).all(|(k, v)| k == v));
                attachment.insert(&k, &k);
            }
            query
                .reply(
                    query.key_expr().clone(),
                    query.value().unwrap().payload.clone(),
                )
                .attachment(attachment)
                .res()
                .unwrap();
        })
        .res()
        .unwrap();
    for i in 0..10 {
        let mut backer = [(
            [0; std::mem::size_of::<usize>()],
            [0; std::mem::size_of::<usize>()],
        ); 10];
        for (j, backer) in backer.iter_mut().enumerate() {
            *backer = ((i * 10 + j).to_le_bytes(), (i * 10 + j).to_be_bytes())
        }
        let get = zenoh
            .get("test/attachment")
            .payload("query")
            .attachment(Some(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            ))
            .res()
            .unwrap();
        while let Ok(reply) = get.recv() {
            let response = reply.sample.as_ref().unwrap();
            for (k, v) in response.attachment().unwrap() {
                assert_eq!(k, v)
            }
        }
    }
}
