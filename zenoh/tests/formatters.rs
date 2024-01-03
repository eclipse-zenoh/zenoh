#[test]
fn reuse() {
    zenoh::kedefine!(
        pub gkeys: "zenoh/${group:*}/${member:*}",
    );
    let mut formatter = gkeys::formatter();
    let k1 = zenoh::keformat!(formatter, group = "foo", member = "bar").unwrap();
    assert_eq!(dbg!(k1).as_str(), "zenoh/foo/bar");

    formatter.set("member", "*").unwrap();
    let k2 = formatter.build().unwrap();
    assert_eq!(dbg!(k2).as_str(), "zenoh/foo/*");

    dbg!(&mut formatter).group("foo").unwrap();
    dbg!(&mut formatter).member("*").unwrap();
    let k2 = dbg!(&mut formatter).build().unwrap();
    assert_eq!(dbg!(k2).as_str(), "zenoh/foo/*");

    let k3 = zenoh::keformat!(formatter, group = "foo", member = "*").unwrap();
    assert_eq!(dbg!(k3).as_str(), "zenoh/foo/*");

    zenoh::keformat!(formatter, group = "**", member = "**").unwrap_err();
}
