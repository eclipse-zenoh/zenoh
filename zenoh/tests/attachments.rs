#[cfg(feature = "unstable")]
#[test]
fn pubsub() {
    use zenoh::prelude::sync::*;

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_subscriber("test/attachments")
        .callback(|sample| {
            println!(
                "{}",
                std::str::from_utf8(&sample.payload.contiguous()).unwrap()
            );
            for (k, v) in &sample.attachments.unwrap() {
                assert!(k.iter().rev().zip(v.as_slice()).all(|(k, v)| k == v))
            }
        })
        .res()
        .unwrap();
    let publisher = zenoh.declare_publisher("test/attachments").res().unwrap();
    for i in 0..10 {
        let mut backer = [(
            [0; std::mem::size_of::<usize>()],
            [0; std::mem::size_of::<usize>()],
        ); 10];
        for (j, backer) in backer.iter_mut().enumerate() {
            *backer = ((i * 10 + j).to_le_bytes(), (i * 10 + j).to_be_bytes())
        }
        zenoh
            .put("test/attachments", "put")
            .with_attachments(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            )
            .res()
            .unwrap();
        publisher
            .put("publisher")
            .with_attachments(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            )
            .res()
            .unwrap();
    }
}
#[cfg(feature = "unstable")]
#[test]
fn queries() {
    use zenoh::{prelude::sync::*, sample::Attachments};

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let _sub = zenoh
        .declare_queryable("test/attachments")
        .callback(|query| {
            println!(
                "{}",
                std::str::from_utf8(
                    &query
                        .value()
                        .map(|q| q.payload.contiguous())
                        .unwrap_or_default()
                )
                .unwrap()
            );
            let mut attachments = Attachments::new();
            for (k, v) in query.attachments().unwrap() {
                assert!(k.iter().rev().zip(v.as_slice()).all(|(k, v)| k == v));
                attachments.insert(&k, &k);
            }
            query
                .reply(Ok(Sample::new(
                    query.key_expr().clone(),
                    query.value().unwrap().clone(),
                )
                .with_attachments(attachments)))
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
            .get("test/attachments")
            .with_value("query")
            .with_attachments(
                backer
                    .iter()
                    .map(|b| (b.0.as_slice(), b.1.as_slice()))
                    .collect(),
            )
            .res()
            .unwrap();
        while let Ok(reply) = get.recv() {
            let response = reply.sample.as_ref().unwrap();
            for (k, v) in response.attachments().unwrap() {
                assert_eq!(k, v)
            }
        }
    }
}
