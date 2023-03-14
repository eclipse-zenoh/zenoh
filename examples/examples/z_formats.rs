use zenoh::prelude::keyexpr;

zenoh::declare_format!(pub format1: "user_id/${user_id:*}/file/${file:**}", pub(crate) format2: "user_id/${user_id:*}/settings/${setting:*/**}");

fn main() {
    // Formatting
    let mut formatter = format1::formatter();
    let file = "hi/there";
    let ke = zenoh::keformat!(formatter, user_id = 42, file).unwrap();
    println!("{formatter:?} => {ke}");
    // Parsing
    let setting_ke = zenoh::key_expr::keyexpr::new("user_id/30/settings/dark_mode").unwrap();
    let parsed = format2::parse(setting_ke).unwrap();
    assert_eq!(parsed.user_id(), keyexpr::new("30").ok());
    assert_eq!(parsed.setting(), keyexpr::new("dark_mode").ok());
}
