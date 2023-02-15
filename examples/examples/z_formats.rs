use zenoh::keformat;
fn main() {
    let format = keformat!("a/${a:*}/b/${b:**}");
    let mut formatter = format.formatter();
    formatter.set("a", "a").unwrap();
    println!("{formatter:?}");
}
