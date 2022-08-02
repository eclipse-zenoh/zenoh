use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use zenoh::config::Config;
use zenoh_core::AsyncResolve;
use zenoh_ext::group::*;

#[async_std::main]
async fn main() {
    env_logger::init();
    let z = Arc::new(zenoh::open(Config::default()).res().await.unwrap());
    let member = Member::new(z.zid().to_string())
        .unwrap()
        .lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), "zgroup", member).await.unwrap();
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
