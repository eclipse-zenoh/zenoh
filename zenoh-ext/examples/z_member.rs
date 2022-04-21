use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use zenoh::config::Config;
use zenoh_ext::group::*;

#[zenoh_async_rt::main]
async fn main() {
    env_logger::init();
    let z = Arc::new(zenoh::open(Config::default()).await.unwrap());
    let member = Member::new(&z.id().await).lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), "zgroup", member).await;
    let rx = group.subscribe().await;
    let mut stream = rx.stream();
    while let Some(evt) = stream.next().await {
        println!(">>> {:?}", &evt);
        println!(">> Group View <<");
        let v = group.view().await;
        println!(
            "{}",
            v.iter()
                .fold(String::from("\n"), |a, b| format!("\t{} \n\t{:?}", a, b)),
        );
        println!(">>>>>>><<<<<<<<<");
    }
}
