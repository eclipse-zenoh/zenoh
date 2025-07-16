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
use zenoh_result::{zerror, ZResult};

#[test]
fn error_simple() {
    let err: ZResult<()> = Err(zerror!("TEST").into());
    if let Err(e) = err {
        let s = e.to_string();
        println!("{e}");
        println!("{e:?}");
        assert!(s.contains("TEST"));
        assert!(s.contains(file!()));
    // assert!(e.source().is_none());
    } else {
        panic!();
    }
}

#[test]
fn error_with_source() {
    let err1: ZResult<()> = Err(zerror!("ERR1").into());
    if let Err(e) = err1 {
        let e = zerror!(e => "ERR2");
        let s = e.to_string();
        println!("{e}");
        println!("{e:?}");

        assert!(s.contains(file!()));
        // assert!(e.source().is_some());
        assert!(s.contains("ERR1"));
        assert!(s.contains("ERR2"));
    } else {
        panic!();
    }

    let ioerr = std::io::Error::other("IOERR");
    let e = zerror!( ioerr =>"ERR2");
    let s = e.to_string();
    println!("{e}");
    println!("{e:?}");

    assert!(s.contains(file!()));
    // assert!(e.source().is_some());
    assert!(s.contains("IOERR"));
    assert!(s.contains("ERR2"));
}
