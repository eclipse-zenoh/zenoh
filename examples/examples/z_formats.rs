use zenoh::keformat;

keformat!("a/${a:*}/b/${b:**}", format);
fn main() {
    let format = format::Format::new();
    let mut formatter = format.formatter();
    keformat!(formatter, a = 1, b = "hi/there").unwrap();
    println!("{formatter:?}");
}
