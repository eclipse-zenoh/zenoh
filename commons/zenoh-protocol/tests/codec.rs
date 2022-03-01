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

use zenoh_buffers::{reader::HasReader, WBuf, ZBuf};
use zenoh_protocol::io::{WBufCodec, ZBufCodec};
use zenoh_protocol_core::{ZInt, ZINT_MAX_BYTES};

fn test_zint(v: ZInt) {
    let mut buf = WBuf::new(32, true);
    buf.write_zint(v);
    dbg!(&buf);
    assert_eq!(v, ZBuf::from(buf).reader().read_zint().unwrap());
}

#[test]
fn test_zint_codec_limits() {
    test_zint(0);
    let one: ZInt = 1;
    for i in 1..ZINT_MAX_BYTES {
        let res = one.checked_shl(7 * i as u32);
        if let Some(v) = res {
            test_zint(v - 1);
            test_zint(v);
        } else {
            break;
        }
    }
    test_zint(dbg!(ZInt::MAX));
}
