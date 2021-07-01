use async_std::sync::Arc;
use std::env::args;
use std::str::FromStr;
use std::time::Duration;
use zenoh::net::*;
use zenoh_ext::net::group::*;

#[async_std::main]
async fn main() {
    env_logger::init();
    let args = args();
    let n: usize = if args.len() >= 2 {
        let argv: Vec<String> = args.collect();
        usize::from_str(argv[1].as_str()).unwrap()
    } else {
        3
    };

    let z = Arc::new(open(ConfigProperties::default()).await.unwrap());
    let mut member = Member::new(&z.id().await);
    member.lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), "zgroup", &member).await;
    if group.wait_for_view_size(n, Duration::from_secs(15)).await {
        println!("Established view size of {}", n);
    } else {
        println!("Failed to establish view size of {}", n);
    }
}
