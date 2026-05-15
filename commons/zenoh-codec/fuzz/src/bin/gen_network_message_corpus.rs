use zenoh_codec_fuzz::write_network_seed_corpus;

fn main() -> std::io::Result<()> {
    for path in write_network_seed_corpus()? {
        println!("{}", path.display());
    }

    Ok(())
}
