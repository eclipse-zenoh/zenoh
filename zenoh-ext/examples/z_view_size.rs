use async_std::sync::Arc;
use clap::{App, Arg};
use std::time::Duration;
use zenoh::prelude::*;
use zenoh_ext::group::*;

#[async_std::main]
async fn main() {
    env_logger::init();

    let (config, group_name, id, size, timeout) = parse_args();

    let z = Arc::new(zenoh::open(config).await.unwrap());
    let member_id = id.unwrap_or(z.id().await);
    let member = Member::new(&member_id).lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), &group_name, member).await;
    println!(
        "Member {} waiting for {} members in group {} for {} seconds...",
        member_id, size, group_name, timeout
    );
    if group
        .wait_for_view_size(size, Duration::from_secs(timeout))
        .await
    {
        println!("Established view size of {} with members:", size);
        for m in group.view().await {
            println!(" - {}", m.id());
        }
    } else {
        println!("Failed to establish view size of {}", size);
    }
}

fn parse_args() -> (Properties, String, Option<String>, usize, u64) {
    let args = App::new("zenoh-ext group view size example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "-g, --group=[STRING] 'The group name'",
        ).default_value("zgroup"))
        .arg(Arg::from_usage(
            "-i, --id=[STRING] 'The group member id (default is the zenoh UUID)'",
        ))
        .arg(Arg::from_usage(
            "-s, --size=[INT] 'The expected group size. The example will wait for the group to reach this size'",
        ).default_value("3"))
        .arg(Arg::from_usage(
            "-t, --timeout=[SEC] 'The duration (in seconds) this example will wait for the group to reach the expected size.'",
        ).default_value("15"))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::from(std::fs::read_to_string(conf_file).unwrap())
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }

    let group = args.value_of("group").unwrap().to_string();
    let id = args.value_of("id").map(String::from);
    let size: usize = args.value_of("size").unwrap().parse().unwrap();
    let timeout: u64 = args.value_of("timeout").unwrap().parse().unwrap();

    (config, group, id, size, timeout)
}
