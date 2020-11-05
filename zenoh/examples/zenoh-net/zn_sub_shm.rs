#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

#[cfg(feature = "zero-copy")]
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let z = open(Properties::default().into()).await.unwrap();
    let id = z.id().await;
    println!("Creating SHM");
    let mut shm = SharedMemoryManager::new(id, 8192).unwrap();
    println!("{:?}", &shm);

    let mut sub = z
        .declare_subscriber(&"/zenoh/shm/data".into(), &SubInfo::default())
        .await
        .unwrap();

    let stream = sub.stream();

    while let Ok(sample) = stream.recv().await {
        let sbuf = sample.payload.into_shm(&mut shm).unwrap();
        println!("{:?}", &shm);
        let bs = sbuf.as_slice();
        for b in bs {
            print!("{:},", b);
        }
        println!(" - ");
        drop(sbuf);
        println!("{:?}", &shm);
    }

    Ok(())
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"tcp udp zero-copy\"\n"
    );
}
