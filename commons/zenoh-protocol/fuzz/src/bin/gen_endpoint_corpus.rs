use zenoh_protocol_fuzz::write_endpoint_seed_corpus;

fn main() {
    let written = write_endpoint_seed_corpus().expect("endpoint corpus generation should succeed");
    for path in written {
        println!("{}", path.display());
    }
}
