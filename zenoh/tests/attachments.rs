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
fn attachment_pubsub() {
    use zenoh::bytes::ZBytes;
    use zenoh::prelude::sync::*;
    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_subscriber("test/attachment")
        .callback(|sample| {
            println!("{}", sample.payload().deserialize::<String>().unwrap());
            for (k, v) in sample.attachment().unwrap().iter::<(
                [u8; std::mem::size_of::<usize>()],
                [u8; std::mem::size_of::<usize>()],
            )>() {
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
            .attachment(ZBytes::from_iter(backer.iter()))
            .res()
            .unwrap();
        publisher
            .put("publisher")
            .attachment(ZBytes::from_iter(backer.iter()))
            .res()
            .unwrap();
    }
}

#[cfg(feature = "unstable")]
#[test]
fn attachment_queries() {
    use zenoh::prelude::sync::*;
    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_queryable("test/attachment")
        .callback(|query| {
            let s = query
                .value()
                .map(|q| q.payload().deserialize::<String>().unwrap())
                .unwrap_or_default();
            println!("Query value: {}", s);

            let attachment = query.attachment().unwrap();
            println!("Query attachment: {:?}", attachment);
            for (k, v) in attachment.iter::<(
                [u8; std::mem::size_of::<usize>()],
                [u8; std::mem::size_of::<usize>()],
            )>() {
                assert!(k.iter().rev().zip(v.as_slice()).all(|(k, v)| k == v));
            }

            query
                .reply(
                    query.key_expr().clone(),
                    query.value().unwrap().payload().clone(),
                )
                .attachment(ZBytes::from_iter(
                    attachment
                        .iter::<(
                            [u8; std::mem::size_of::<usize>()],
                            [u8; std::mem::size_of::<usize>()],
                        )>()
                        .map(|(k, _)| (k, k)),
                ))
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
            .attachment(ZBytes::from_iter(backer.iter()))
            .res()
            .unwrap();
        while let Ok(reply) = get.recv() {
            let response = reply.result().unwrap();
            for (k, v) in response.attachment().unwrap().iter::<(
                [u8; std::mem::size_of::<usize>()],
                [u8; std::mem::size_of::<usize>()],
            )>() {
                assert_eq!(k, v)
            }
        }
    }
}
