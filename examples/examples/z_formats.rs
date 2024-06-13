//
// Copyright (c) 2023 ZettaScale Technology
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

use zenoh::key_expr::{
    format::{kedefine, keformat},
    keyexpr,
};
kedefine!(
    pub file_format: "user_id/${user_id:*}/file/${file:*/**}",
    pub(crate) settings_format: "user_id/${user_id:*}/settings/${setting:**}"
);

fn main() {
    // Formatting
    let mut formatter = file_format::formatter();
    let file = "hi/there";
    let ke = keformat!(formatter, user_id = 42, file).unwrap();
    println!("{formatter:?} => {ke}");
    // Parsing
    let settings_ke = keyexpr::new("user_id/30/settings/dark_mode").unwrap();
    let parsed = settings_format::parse(settings_ke).unwrap();
    assert_eq!(parsed.user_id(), keyexpr::new("30").unwrap());
    assert_eq!(parsed.setting(), keyexpr::new("dark_mode").ok());
}
