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
use zenoh_protocol::proto::{
  SeqNum,
  SeqNumGenerator
};


#[test]
fn sn_pre_test() {
  let sn0a = SeqNum::make(0, 14).unwrap();
  let sn1a: ZInt = 1;
  assert!(sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(0, 14).unwrap();
  let sn1a: ZInt = 0;
  assert!(!sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(0, 14).unwrap();
  let sn1a: ZInt = 6;
  assert!(sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(0, 14).unwrap();
  let sn1a: ZInt = 7;
  assert!(sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(13, 14).unwrap();
  let sn1a: ZInt = 6;
  assert!(!sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(13, 14).unwrap();
  let sn1a: ZInt = 1;
  assert!(sn0a.precedes(sn1a));

  let sn0a = SeqNum::make(13, 14).unwrap();
  let sn1a: ZInt = 5;
  assert!(sn0a.precedes(sn1a));
}

#[test]
fn sn_gen_test() {
  let mut sn0 = SeqNumGenerator::make(13, 14).unwrap();
  let mut sn1 = SeqNumGenerator::make(5, 14).unwrap();

  let sn = sn0.get();
  assert_eq!(sn, 13);
  let sn = sn1.get();
  assert_eq!(sn, 5);

  let sn = sn0.get();
  assert_eq!(sn, 0);
  let sn = sn1.get();
  assert_eq!(sn, 6);
}