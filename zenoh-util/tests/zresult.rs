//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

#[test]
fn error_simple() {
    let err: ZResult<()> = zerror!(ZErrorKind::Other {
        descr: "TEST".to_string()
    });
    if let Err(e) = err {
        let s = e.to_string();
        println!("{}", e);
        println!("{:?}", e);
        assert!(if let ZErrorKind::Other { descr: _ } = e.get_kind() {
            true
        } else {
            false
        });
        assert!(s.contains("TEST"));
        assert!(s.contains(file!()));
    // assert!(e.source().is_none());
    } else {
        assert!(false);
    }

    let err: ZResult<()> = zerror!(ZErrorKind::BufferOverflow { missing: 3 });
    if let Err(e) = err {
        let s = e.to_string();
        println!("{}", e);
        println!("{:?}", e);
        match e.get_kind() {
            ZErrorKind::BufferOverflow { missing: x } => assert_eq!(3usize, *x),
            _ => assert!(false),
        }
        assert!(s.contains(file!()));
    // assert!(e.source().is_none());
    } else {
        assert!(false);
    }
}

#[test]
fn error_with_source() {
    let err1: ZResult<()> = zerror!(ZErrorKind::Other {
        descr: "ERR1".to_string()
    });
    if let Err(e) = err1 {
        let err2: ZResult<()> = zerror!(
            ZErrorKind::Other {
                descr: "ERR2".to_string()
            },
            e
        );
        if let Err(e) = err2 {
            let s = e.to_string();
            println!("{}", e);
            println!("{:?}", e);

            assert!(if let ZErrorKind::Other { descr: _ } = e.get_kind() {
                true
            } else {
                false
            });
            assert!(s.contains(file!()));
            // assert!(e.source().is_some());
            assert_eq!(true, s.contains("ERR1"));
            assert_eq!(true, s.contains("ERR2"));
        } else {
            assert!(false);
        }
    } else {
        assert!(false);
    }

    let ioerr = std::io::Error::new(std::io::ErrorKind::Other, "IOERR");
    let err2: ZResult<()> = zerror!(
        ZErrorKind::Other {
            descr: "ERR2".to_string()
        },
        ioerr
    );
    if let Err(e) = err2 {
        let s = e.to_string();
        println!("{}", e);
        println!("{:?}", e);

        assert!(if let ZErrorKind::Other { descr: _ } = e.get_kind() {
            true
        } else {
            false
        });
        assert!(s.contains(file!()));
        // assert!(e.source().is_some());
        assert_eq!(true, s.contains("IOERR"));
        assert_eq!(true, s.contains("ERR2"));
    } else {
        assert!(false);
    }
}
