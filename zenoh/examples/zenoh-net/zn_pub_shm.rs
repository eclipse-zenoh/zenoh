#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

#[cfg(feature = "zero-copy")]
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let z = open(Properties::default().into()).await.unwrap();
    let id = z.id().await;
    let mut shm = SharedMemoryManager::new(id, 8192).unwrap();
    println!("{:?}", &shm);
    println!("Created Shared Memory Manager");
    let mut sbuf = shm.alloc(1024).unwrap();
    let bs = unsafe { sbuf.as_mut_slice() };
    for b in bs {
        *b = rand::random::<u8>();
    }
    println!("{:?}", &shm);
    z.write(&"/zenoh/shm/data".into(), sbuf.into()).await?;
    println!("{:?}", &shm);
    let mut buffer = String::new();
    println!("Press any key to exit...");
    std::io::stdin().read_line(&mut buffer)?;
    let freed = shm.garbage_collect();
    println!("GC freed: {:?}", freed);
    println!("{:?}", &shm);
    Ok(())
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"tcp udp zero-copy\"\n"
    );
}
