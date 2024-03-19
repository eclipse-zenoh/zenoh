use config::ConnectionRetryConf;

use zenoh::prelude::sync::*;

#[test]
fn retry_config_overriding() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/endpoints",
            r#"
            [
                "tcp/1.2.3.4:0",
                "tcp/1.2.3.4:0#retry_period_init_ms=30000",
                "tcp/1.2.3.4:0#retry_period_init_ms=30000;retry_period_max_ms=60000;retry_period_increase_factor=15;exit_on_failure=true",
            ]
            "#,
        )
        .unwrap();

    config
        .insert_json5(
            "listen/retry",
            r#"
            { 
                try_timeout_ms: 2000,
                period_init_ms: 3000,
                period_max_ms: 6000,
                period_increase_factor: 1.5,
            }
            "#,
        )
        .unwrap();

    config
        .insert_json5("listen/exit_on_failure", "false")
        .unwrap();

    let expected = vec![
        // global value
        ConnectionRetryConf {
            period_init_ms: 3000,
            period_max_ms: 6000,
            period_increase_factor: 1.5,
            exit_on_failure: false,
        },
        // override one key
        ConnectionRetryConf {
            period_init_ms: 30000,
            period_max_ms: 6000,
            period_increase_factor: 1.5,
            exit_on_failure: false,
        },
        // override all keys
        ConnectionRetryConf {
            period_init_ms: 30000,
            period_max_ms: 60000,
            period_increase_factor: 15.,
            exit_on_failure: true,
        },
    ];

    for (i, endpoint) in config.listen().endpoints().iter().enumerate() {
        let retry_config = zenoh_config::get_retry_config(&config, Some(endpoint), true);
        assert_eq!(retry_config, expected[i]);
    }
}

#[test]
fn retry_config_parsing() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/retry",
            r#"
            { 
                period_init_ms: 1000,
                period_max_ms: 6000,
                period_increase_factor: 2,
            }
            "#,
        )
        .unwrap();

    let endpoint: EndPoint = "tcp/[::]:0".parse().unwrap();
    let retry_config = zenoh_config::get_retry_config(&config, Some(&endpoint), true);

    let mut period = retry_config.period();
    let expected = vec![1000, 2000, 4000, 6000, 6000, 6000, 6000];

    for v in expected {
        assert_eq!(period.duration(), std::time::Duration::from_millis(v));
        assert_eq!(period.next_duration(), std::time::Duration::from_millis(v));
    }
}

#[test]
fn retry_config_const_period() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/retry",
            r#"
            { 
                period_init_ms: 1000,
                period_increase_factor: 1,
            }
            "#,
        )
        .unwrap();

    let endpoint: EndPoint = "tcp/[::]:0".parse().unwrap();
    let retry_config = zenoh_config::get_retry_config(&config, Some(&endpoint), true);

    let mut period = retry_config.period();
    let expected = vec![1000, 1000, 1000, 1000];

    for v in expected {
        assert_eq!(period.duration(), std::time::Duration::from_millis(v));
        assert_eq!(period.next_duration(), std::time::Duration::from_millis(v));
    }
}

#[test]
fn retry_config_infinit_period() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/retry",
            r#"
            { 
                period_init_ms: -1,
                period_increase_factor: 1,
            }
            "#,
        )
        .unwrap();

    let endpoint: EndPoint = "tcp/[::]:0".parse().unwrap();
    let retry_config = zenoh_config::get_retry_config(&config, Some(&endpoint), true);

    let mut period = retry_config.period();

    assert_eq!(period.duration(), std::time::Duration::MAX);
    assert_eq!(period.next_duration(), std::time::Duration::MAX);
}

#[test]
#[should_panic(expected = "Can not create a new TCP listener")]
fn listen_no_retry() {
    let mut config = Config::default();
    config
        .insert_json5("listen/endpoints", r#"["tcp/8.8.8.8:8"]"#)
        .unwrap();

    config.insert_json5("listen/timeout_ms", "0").unwrap();
    zenoh::open(config).res().unwrap();
}

#[test]
#[should_panic(expected = "value: Elapsed(())")]
fn listen_with_retry() {
    let mut config = Config::default();
    config
        .insert_json5("listen/endpoints", r#"["tcp/8.8.8.8:8"]"#)
        .unwrap();

    config.insert_json5("listen/timeout_ms", "1000").unwrap();

    zenoh::open(config).res().unwrap();
}
