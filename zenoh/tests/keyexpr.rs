//
// Copyright (c) 2025 ZettaScale Technology
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
use zenoh::key_expr::KeyExpr;
#[cfg(feature = "internal_config")]
use zenoh::Wait;
#[cfg(feature = "internal_config")]
use zenoh_config::WhatAmI;

#[test]
fn keyexpr_test_from_string_unchecked() {
    let unsafe_expr = unsafe { KeyExpr::from_string_unchecked("expr".to_string()) };
    let safe_expr = KeyExpr::try_from("expr").unwrap();
    assert_eq!(unsafe_expr, safe_expr);
}

#[test]
fn keyexpr_test_from_boxed_str_unchecked() {
    let unsafe_expr = unsafe { KeyExpr::from_boxed_str_unchecked("expr".into()) };
    let safe_expr = KeyExpr::try_from("expr").unwrap();
    assert_eq!(unsafe_expr, safe_expr);
}

#[test]
fn keyexpr_test_from_str_unchecked() {
    let unsafe_expr = unsafe { KeyExpr::from_str_unchecked("expr") };
    let safe_expr = KeyExpr::try_from("expr").unwrap();
    assert_eq!(unsafe_expr, safe_expr);
}

#[test]
fn keyexpr_test_new() {
    let expr = KeyExpr::new("expr").unwrap();
    assert_eq!(expr.as_str(), "expr");
}

#[test]
#[cfg(feature = "internal_config")]
fn keyexpr_test_into_owned_borrowing_clone() {
    let expr = KeyExpr::try_from("expr").unwrap();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    let expr = expr.into_owned();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    let expr = expr.into_owned();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let expr = session.declare_keyexpr("expr").wait().unwrap();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    let expr = expr.into_owned();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    let expr = expr.into_owned();
    let clone = expr.borrowing_clone();
    assert_eq!(expr, clone);

    session.close().wait().unwrap();
}

#[test]
fn keyexpr_test_autocannonize() {
    let expr = KeyExpr::autocanonize("some/expr/**/**".to_string()).unwrap();
    assert_eq!(expr.as_str(), "some/expr/**");
}

#[test]
#[cfg(feature = "internal_config")]
fn keyexpr_test_join() {
    let prefix = KeyExpr::try_from("some/prefix").unwrap();
    let suffix = KeyExpr::try_from("some/suffix").unwrap();
    let join = prefix.join(&suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let wire_prefix = session.declare_keyexpr("some/prefix").wait().unwrap();
    let wire_suffix = session.declare_keyexpr("some/suffix").wait().unwrap();
    let join = wire_prefix.join(&wire_suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let join = prefix.join(&wire_suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let join = wire_prefix.join(&suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let owned_wire_prefix = wire_prefix.into_owned();
    let owned_wire_suffix = wire_suffix.into_owned();

    let join = owned_wire_prefix.join(&owned_wire_suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let join = prefix.join(&owned_wire_suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");

    let join = owned_wire_prefix.join(&suffix).unwrap();
    assert_eq!(join.as_str(), "some/prefix/some/suffix");
}

#[test]
#[cfg(feature = "internal_config")]
fn keyexpr_test_concat() {
    let prefix = KeyExpr::try_from("some/prefix").unwrap();
    let suffix = KeyExpr::try_from("some/suffix").unwrap();
    let concat = prefix.concat(&suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let wire_prefix = session.declare_keyexpr("some/prefix").wait().unwrap();
    let wire_suffix = session.declare_keyexpr("some/suffix").wait().unwrap();
    let concat = wire_prefix.concat(&wire_suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let concat = prefix.concat(&wire_suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let concat = wire_prefix.concat(&suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let owned_wire_prefix = wire_prefix.into_owned();
    let owned_wire_suffix = wire_suffix.into_owned();

    let concat = owned_wire_prefix.concat(&owned_wire_suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let concat = prefix.concat(&owned_wire_suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let concat = owned_wire_prefix.concat(&suffix).unwrap();
    assert_eq!(concat.as_str(), "some/prefixsome/suffix");

    let prefix = KeyExpr::try_from("some/*").unwrap();
    let suffix = KeyExpr::try_from("*/suffix").unwrap();
    assert!(prefix.concat(&suffix).is_err());
}

#[test]
fn keyexpr_test_from_str() {
    use std::str::FromStr;
    assert!(KeyExpr::from_str("expr").is_ok());
    assert!(KeyExpr::from_str("/expr").is_err());
    assert!(KeyExpr::from_str("expr/").is_err());
    assert!(KeyExpr::from_str("expr//expr").is_err());
}

#[test]
fn keyexpr_test_from() {
    let expr = KeyExpr::try_from("expr").unwrap();
    let string = String::from(expr);
    assert_eq!(string.as_str(), "expr");

    let expr = KeyExpr::try_from("expr").unwrap().into_owned();
    let string = String::from(expr);
    assert_eq!(string.as_str(), "expr");
}

#[test]
fn keyexpr_test_try_from() {
    assert!(KeyExpr::try_from("expr").is_ok());
    assert!(KeyExpr::try_from("/expr").is_err());
    assert!(KeyExpr::try_from(&"expr".to_string()).is_ok());
    assert!(KeyExpr::try_from(&"/expr".to_string()).is_err());
    assert!(KeyExpr::try_from(&mut "expr".to_string()).is_ok());
    assert!(KeyExpr::try_from(&mut "/expr".to_string()).is_err());
    assert!(KeyExpr::try_from("expr".to_string().as_mut_str()).is_ok());
    assert!(KeyExpr::try_from("/expr".to_string().as_mut_str()).is_err());
}

#[test]
fn keyexpr_test_as_ref() {
    let expr = KeyExpr::try_from("expr").unwrap();
    assert_eq!(
        AsRef::<zenoh_keyexpr::keyexpr>::as_ref(&expr),
        zenoh_keyexpr::keyexpr::new("expr").unwrap()
    );
}

#[test]
fn keyexpr_test_debug() {
    let expr = KeyExpr::try_from("expr").unwrap();
    assert_eq!(format!("{expr:?}"), "ke`expr`");
}

#[test]
#[cfg(feature = "internal_config")]
fn keyexpr_test_div() {
    let suffix = KeyExpr::try_from("suffix").unwrap();

    let prefix = KeyExpr::try_from("prefix").unwrap();
    assert_eq!(
        &prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );
    assert_eq!(
        prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );

    let prefix = KeyExpr::try_from("prefix").unwrap().into_owned();
    assert_eq!(
        &prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );
    assert_eq!(
        prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = zenoh::open(config).wait().unwrap();

    let wire_prefix = session.declare_keyexpr("prefix").wait().unwrap();
    assert_eq!(
        &wire_prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );
    assert_eq!(
        wire_prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );

    let wire_prefix = session
        .declare_keyexpr("prefix")
        .wait()
        .unwrap()
        .into_owned();
    assert_eq!(
        &wire_prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );
    assert_eq!(
        wire_prefix / &suffix,
        KeyExpr::try_from("prefix/suffix").unwrap()
    );

    session.close().wait().unwrap();
}

#[test]
#[cfg(feature = "internal_config")]
fn keyexpr_test_undeclare() {
    let expr = KeyExpr::try_from("expr").unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let wire_expr = session.declare_keyexpr("expr").wait().unwrap();
    let owned_wire_expr = session.declare_keyexpr("expr").wait().unwrap().into_owned();

    assert!(session.undeclare(wire_expr).wait().is_ok());
    assert!(session.undeclare(owned_wire_expr).wait().is_ok());
    assert!(session.undeclare(expr).wait().is_err());

    // let mut config = zenoh::Config::default();
    // config.set_mode(Some(WhatAmI::Peer)).unwrap();
    // config.listen.endpoints.set(vec![]).unwrap();
    // config.scouting.multicast.set_enabled(Some(false)).unwrap();
    // let session2 = zenoh::open(config).wait().unwrap();
    // let wire_expr2 = session2.declare_keyexpr("expr").wait().unwrap();
    // assert!(session.undeclare(wire_expr2).wait().is_err());
    // session2.close().wait().unwrap();

    session.close().wait().unwrap();
}

#[test]
#[cfg(feature = "internal")]
fn keyexpr_test_dummy() {
    let dummy_expr = KeyExpr::dummy();
    assert!(dummy_expr.is_dummy());

    let non_dummy_expr = KeyExpr::try_from("dummy").unwrap();
    assert!(!non_dummy_expr.is_dummy());
}
