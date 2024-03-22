#[test]
fn pubsub_with_ringbuffer() {
    use zenoh::{handlers::RingBuffer, prelude::sync::*};

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let sub = zenoh
        .declare_subscriber("test/ringbuffer")
        .with(RingBuffer::new(3))
        .res()
        .unwrap();
    for i in 0..10 {
        zenoh
            .put("test/ringbuffer", format!("put{i}"))
            .res()
            .unwrap();
    }
    // Should only receive the last three samples ("put7", "put8", "put9")
    for i in 7..10 {
        assert_eq!(
            sub.recv()
                .unwrap()
                .unwrap()
                .payload()
                .deserialize::<String>()
                .unwrap(),
            format!("put{i}")
        );
    }
}

#[test]
fn query_with_ringbuffer() {
    use zenoh::{handlers::RingBuffer, prelude::sync::*};

    let zenoh = zenoh::open(Config::default()).res().unwrap();
    let queryable = zenoh
        .declare_queryable("test/ringbuffer_query")
        .with(RingBuffer::new(1))
        .res()
        .unwrap();

    let _reply1 = zenoh
        .get("test/ringbuffer_query")
        .with_value("query1")
        .res()
        .unwrap();
    let _reply2 = zenoh
        .get("test/ringbuffer_query")
        .with_value("query2")
        .res()
        .unwrap();

    let query = queryable.recv().unwrap().unwrap();
    // Only receive the latest query
    assert_eq!(
        query
            .value()
            .unwrap()
            .payload
            .deserialize::<String>()
            .unwrap(),
        "query2"
    );
}
