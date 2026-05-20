use zenoh_codec_fuzz::write_all_seed_corpora;

fn main() {
    let written = write_all_seed_corpora().expect("all corpus generation should succeed");
    for path in written {
        println!("{}", path.display());
    }
}
