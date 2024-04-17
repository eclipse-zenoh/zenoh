//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
#[test]
fn kedefine_reuse() {
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
