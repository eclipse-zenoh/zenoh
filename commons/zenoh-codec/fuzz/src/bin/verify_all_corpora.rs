use zenoh_codec_fuzz::verify_all_seed_corpora;

fn main() {
    verify_all_seed_corpora().expect("all corpus verification should succeed");
}
