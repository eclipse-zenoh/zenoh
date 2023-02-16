use zenoh::keformat;

keformat!("a/${a:*}/b/${b:**}", format);
fn main() {
    let mut formatter = format::Format::formatter();
    keformat!(formatter, a = 1, b = "hi/there").unwrap();
    println!("{formatter:?}");
}
