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
use std::sync::{Arc, Mutex};

use zenoh::{internal::zlock, prelude::*};

#[cfg(target_os = "windows")]
static MINIMAL_SLEEP_INTERVAL_MS: u64 = 17;
#[cfg(not(target_os = "windows"))]
static MINIMAL_SLEEP_INTERVAL_MS: u64 = 2;

struct IntervalCounter {
    first_tick: bool,
    last_time: std::time::Instant,
    count: u32,
    total_time: std::time::Duration,
}

impl IntervalCounter {
    fn new() -> IntervalCounter {
        IntervalCounter {
            first_tick: true,
            last_time: std::time::Instant::now(),
            count: 0,
            total_time: std::time::Duration::from_secs(0),
        }
    }

    fn tick(&mut self) {
        let curr_time = std::time::Instant::now();
        if self.first_tick {
            self.first_tick = false;
        } else {
            self.total_time += curr_time - self.last_time;
            self.count += 1;
        }
        self.last_time = curr_time;
    }

    fn get_middle(&self) -> u32 {
        assert!(self.count > 0);
        self.total_time.as_millis() as u32 / self.count
    }

    fn get_count(&self) -> u32 {
        self.count
    }

    fn check_middle(&self, ms: u32) {
        let middle = self.get_middle();
        println!("Interval {}, count: {}, middle: {}", ms, self.count, middle);
        assert!(middle + 1 >= ms);
    }
}

fn downsampling_by_keyexpr_impl(egress: bool) {
    zenoh_util::try_init_log_from_env();

    let ds_cfg = format!(
        r#"
          [
            {{
              flow: "{}",
              rules: [
                {{ key_expr: "test/downsamples_by_keyexp/r100", freq: 10, }},
                {{ key_expr: "test/downsamples_by_keyexp/r50", freq: 20, }}
              ],
            }},
          ] "#,
        (if egress { "egress" } else { "ingress" })
    );

    // declare subscriber
    let mut config_sub = Config::default();
    if !egress {
        config_sub.insert_json5("downsampling", &ds_cfg).unwrap();
    }
    config_sub
        .insert_json5("listen/endpoints", r#"["tcp/127.0.0.1:38446"]"#)
        .unwrap();
    config_sub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let zenoh_sub = zenoh::open(config_sub).wait().unwrap();

    let counter_r100 = Arc::new(Mutex::new(IntervalCounter::new()));
    let counter_r100_clone = counter_r100.clone();
    let counter_r50 = Arc::new(Mutex::new(IntervalCounter::new()));
    let counter_r50_clone = counter_r50.clone();

    let total_count = Arc::new(Mutex::new(0));
    let total_count_clone = total_count.clone();

    let _sub = zenoh_sub
        .declare_subscriber("test/downsamples_by_keyexp/*")
        .callback(move |sample| {
            let mut count = zlock!(total_count_clone);
            *count += 1;
            if sample.key_expr().as_str() == "test/downsamples_by_keyexp/r100" {
                zlock!(counter_r100).tick();
            } else if sample.key_expr().as_str() == "test/downsamples_by_keyexp/r50" {
                zlock!(counter_r50).tick();
            }
        })
        .wait()
        .unwrap();

    // declare publisher
    let mut config_pub = Config::default();
    if egress {
        config_pub.insert_json5("downsampling", &ds_cfg).unwrap();
    }
    config_pub
        .insert_json5("connect/endpoints", r#"["tcp/127.0.0.1:38446"]"#)
        .unwrap();
    config_pub
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();
    let zenoh_pub = zenoh::open(config_pub).wait().unwrap();
    let publisher_r100 = zenoh_pub
        .declare_publisher("test/downsamples_by_keyexp/r100")
        .wait()
        .unwrap();

    let publisher_r50 = zenoh_pub
        .declare_publisher("test/downsamples_by_keyexp/r50")
        .wait()
        .unwrap();

    let publisher_all = zenoh_pub
        .declare_publisher("test/downsamples_by_keyexp/all")
        .wait()
        .unwrap();

    // WARN(yuyuan): 2 ms is the limit of tokio
    let interval = std::time::Duration::from_millis(MINIMAL_SLEEP_INTERVAL_MS);
    let messages_count = 1000;
    for i in 0..messages_count {
        publisher_r100.put(format!("message {}", i)).wait().unwrap();
        publisher_r50.put(format!("message {}", i)).wait().unwrap();
        publisher_all.put(format!("message {}", i)).wait().unwrap();
        std::thread::sleep(interval);
    }

    for _ in 0..100 {
        if *zlock!(total_count) >= messages_count
            && zlock!(counter_r50_clone).get_count() > 0
            && zlock!(counter_r100_clone).get_count() > 0
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(*zlock!(total_count) >= messages_count);

    zlock!(counter_r50_clone).check_middle(50);
    zlock!(counter_r100_clone).check_middle(100);
}

#[test]
fn downsampling_by_keyexpr() {
    downsampling_by_keyexpr_impl(true);
    downsampling_by_keyexpr_impl(false);
}

#[cfg(unix)]
fn downsampling_by_interface_impl(egress: bool) {
    zenoh_util::try_init_log_from_env();

    let ds_cfg = format!(
        r#"
          [
            {{
              interfaces: ["lo", "lo0"],
              flow: "{0}",
              rules: [
                {{ key_expr: "test/downsamples_by_interface/r100", freq: 10, }},
              ],
            }},
            {{
              interfaces: ["some_unknown_interface"],
              flow: "{0}",
              rules: [
                {{ key_expr: "test/downsamples_by_interface/all", freq: 10, }},
              ],
            }},
          ] "#,
        (if egress { "egress" } else { "ingress" })
    );
    // declare subscriber
    let mut config_sub = Config::default();
    config_sub
        .insert_json5("listen/endpoints", r#"["tcp/127.0.0.1:38447"]"#)
        .unwrap();
    if !egress {
        config_sub.insert_json5("downsampling", &ds_cfg).unwrap();
    };
    let zenoh_sub = zenoh::open(config_sub).wait().unwrap();

    let counter_r100 = Arc::new(Mutex::new(IntervalCounter::new()));
    let counter_r100_clone = counter_r100.clone();

    let total_count = Arc::new(Mutex::new(0));
    let total_count_clone = total_count.clone();

    let _sub = zenoh_sub
        .declare_subscriber("test/downsamples_by_interface/*")
        .callback(move |sample| {
            let mut count = zlock!(total_count_clone);
            *count += 1;
            if sample.key_expr().as_str() == "test/downsamples_by_interface/r100" {
                zlock!(counter_r100).tick();
            }
        })
        .wait()
        .unwrap();

    // declare publisher
    let mut config_pub = Config::default();
    config_pub
        .insert_json5("connect/endpoints", r#"["tcp/127.0.0.1:38447"]"#)
        .unwrap();
    if egress {
        config_pub.insert_json5("downsampling", &ds_cfg).unwrap();
    }
    let zenoh_pub = zenoh::open(config_pub).wait().unwrap();
    let publisher_r100 = zenoh_pub
        .declare_publisher("test/downsamples_by_interface/r100")
        .wait()
        .unwrap();

    let publisher_all = zenoh_pub
        .declare_publisher("test/downsamples_by_interface/all")
        .wait()
        .unwrap();

    // WARN(yuyuan): 2 ms is the limit of tokio
    let interval = std::time::Duration::from_millis(MINIMAL_SLEEP_INTERVAL_MS);
    let messages_count = 1000;
    for i in 0..messages_count {
        publisher_r100.put(format!("message {}", i)).wait().unwrap();
        publisher_all.put(format!("message {}", i)).wait().unwrap();

        std::thread::sleep(interval);
    }

    for _ in 0..100 {
        if *zlock!(total_count) >= messages_count && zlock!(counter_r100_clone).get_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(*zlock!(total_count) >= messages_count);

    zlock!(counter_r100_clone).check_middle(100);
}

#[cfg(unix)]
#[test]
fn downsampling_by_interface() {
    downsampling_by_interface_impl(true);
    downsampling_by_interface_impl(false);
}

#[test]
#[should_panic(expected = "unknown variant `down`")]
fn downsampling_config_error_wrong_strategy() {
    zenoh_util::try_init_log_from_env();

    let mut config = Config::default();
    config
        .insert_json5(
            "downsampling",
            r#"
              [
                {
                  flow: "down",
                  rules: [
                    { keyexpr: "test/downsamples_by_keyexp/r100", freq: 10, },
                    { keyexpr: "test/downsamples_by_keyexp/r50", freq: 20, }
                  ],
                },
              ]
            "#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}
