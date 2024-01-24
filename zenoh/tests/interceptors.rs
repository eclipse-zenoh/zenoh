use std::sync::{Arc, Mutex};

#[test]
fn downsampling() {
    let _ = env_logger::builder().is_test(true).try_init();

    use zenoh::prelude::sync::*;

    // declare publisher
    let mut config = Config::default();
    config
        .insert_json5(
            "downsampling/downsamples",
            r#"
              [
                {
                  keyexpr: "test/downsamples/r100",
                  threshold_ms: 100,
                },
                {
                  keyexpr: "test/downsamples/r50",
                  threshold_ms: 50,
                },
              ]
            "#,
        )
        .unwrap();

    // declare subscriber
    let zenoh_sub = zenoh::open(config).res().unwrap();

    let last_time_r100 = Arc::new(Mutex::new(
        std::time::Instant::now() - std::time::Duration::from_millis(100),
    ));
    let last_time_r50 = Arc::new(Mutex::new(
        std::time::Instant::now() - std::time::Duration::from_millis(50),
    ));

    let _sub = zenoh_sub
        .declare_subscriber("test/downsamples/*")
        .callback(move |sample| {
            let curr_time = std::time::Instant::now();
            if sample.key_expr.as_str() == "test/downsamples/r100" {
                let mut last_time = last_time_r100.lock().unwrap();
                let interval = (curr_time - *last_time).as_millis() + 1;
                *last_time = curr_time;
                println!("interval 100: {}", interval);
                assert!(interval >= 100);
            } else if sample.key_expr.as_str() == "test/downsamples/r50" {
                let mut last_time = last_time_r50.lock().unwrap();
                let interval = (curr_time - *last_time).as_millis() + 1;
                *last_time = curr_time;
                println!("interval 50: {}", interval);
                assert!(interval >= 50);
            }
        })
        .res()
        .unwrap();

    let zenoh_pub = zenoh::open(Config::default()).res().unwrap();
    let publisher_r100 = zenoh_pub
        .declare_publisher("test/downsamples/r100")
        .res()
        .unwrap();

    let publisher_r50 = zenoh_pub
        .declare_publisher("test/downsamples/r50")
        .res()
        .unwrap();

    let interval = std::time::Duration::from_millis(1);
    for i in 0..1000 {
        println!("message {}", i);
        publisher_r100.put(format!("message {}", i)).res().unwrap();
        publisher_r50.put(format!("message {}", i)).res().unwrap();

        std::thread::sleep(interval);
    }
}
