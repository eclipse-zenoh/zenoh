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
use zenoh_protocol::core::ZInt;
use zenoh_protocol::proto::{SeqNum, SeqNumGenerator};

#[test]
fn sn_set() {
    let mut sn0a = SeqNum::new(0, 14);
    assert_eq!(sn0a.get(), 0);
    assert_eq!(sn0a.resolution(), 14);

    let res = sn0a.set(13);
    assert!(res.is_ok());
    assert_eq!(sn0a.get(), 13);

    let res = sn0a.set(14);
    assert!(res.is_err());
    assert_eq!(sn0a.get(), 13);

    sn0a.increment();
    assert_eq!(sn0a.get(), 0);

    sn0a.increment();
    assert_eq!(sn0a.get(), 1);
}

#[test]
fn sn_gap() {
    let mut sn0a = SeqNum::new(0, 14);
    let sn1a: ZInt = 0;
    let res = sn0a.gap(sn1a);
    assert_eq!(res.unwrap(), 0);

    let sn1a: ZInt = 1;
    let res = sn0a.gap(sn1a);
    assert_eq!(res.unwrap(), 1);

    let sn1a: ZInt = 13;
    let res = sn0a.gap(sn1a);
    assert_eq!(res.unwrap(), 13);

    let sn1a: ZInt = 14;
    let res = sn0a.gap(sn1a);
    assert!(res.is_err());

    let res = sn0a.set(13);
    assert!(res.is_ok());

    let sn1a: ZInt = 13;
    let res = sn0a.gap(sn1a);
    assert_eq!(res.unwrap(), 0);

    let sn1a: ZInt = 0;
    let res = sn0a.gap(sn1a);
    assert_eq!(res.unwrap(), 1);
}

#[test]
fn sn_precedence() {
    let mut sn0a = SeqNum::new(0, 14);
    let sn1a: ZInt = 1;
    let res = sn0a.precedes(sn1a);
    assert!(res.unwrap());

    let sn1a: ZInt = 0;
    let res = sn0a.precedes(sn1a);
    assert!(!res.unwrap());

    let sn1a: ZInt = 6;
    let res = sn0a.precedes(sn1a);
    assert!(res.unwrap());

    let sn1a: ZInt = 7;
    let res = sn0a.precedes(sn1a);
    assert!(res.unwrap());

    let res = sn0a.set(13);
    assert!(res.is_ok());

    let sn1a: ZInt = 6;
    let res = sn0a.precedes(sn1a);
    assert!(!res.unwrap());

    let sn1a: ZInt = 1;
    let res = sn0a.precedes(sn1a);
    assert!(res.unwrap());

    let sn1a: ZInt = 5;
    let res = sn0a.precedes(sn1a);
    assert!(res.unwrap());
}

#[test]
fn sn_generation() {
    let mut sn0 = SeqNumGenerator::new(13, 14);
    let mut sn1 = SeqNumGenerator::new(5, 14);

    assert_eq!(sn0.get(), 13);
    assert_eq!(sn1.get(), 5);

    assert_eq!(sn0.get(), 0);
    assert_eq!(sn1.get(), 6);
}