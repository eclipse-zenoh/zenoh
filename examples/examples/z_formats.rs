use zenoh::{keformat, prelude::keyexpr};

keformat!("user_id/${user_id:*}/file/${file:**}", format);

fn main() {
    let mut formatter = format::formatter();
    let file = "hi/there";
    let ke = keformat!(formatter, user_id = 42, file)
        .unwrap()
        .build()
        .unwrap();
    println!("{formatter:?} => {ke}");
    let parsed = format::parse(&ke).unwrap();
    assert_eq!(parsed.user_id(), keyexpr::new("42").ok());
    assert_eq!(parsed.file(), keyexpr::new(file).ok());
}
