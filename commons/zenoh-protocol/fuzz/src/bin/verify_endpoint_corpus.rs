use zenoh_protocol_fuzz::verify_endpoint_seed_corpus;

fn main() {
    verify_endpoint_seed_corpus().expect("endpoint corpus verification should succeed");
}
