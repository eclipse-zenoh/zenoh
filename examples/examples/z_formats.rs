//
// Copyright (c) 2022 ZettaScale Technology
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

use zenoh::prelude::keyexpr;

zenoh::define_format!(pub format1: "user_id/${user_id:*}/file/${file:**}", pub(crate) format2: "user_id/${user_id:*}/settings/${setting:*/**}");

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
