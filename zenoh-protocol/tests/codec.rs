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
use zenoh_protocol::io::{WBuf, RBuf};

use zenoh_util::core::ZResult;


fn test_zint(v: ZInt) -> ZResult<()> {
    let mut buf = WBuf::new(32, true);
    buf.write_zint(v);
    assert_eq!(v, RBuf::from(&buf).read_zint()?);
    Ok(())
}


#[test]
fn test_zint_codec_limits() -> ZResult<()> {
    test_zint(0)?;
    for i in 1 .. 10 {
    let v: ZInt =  1 << (7*i);
    test_zint(v-1)?;
    test_zint(v)?;
    }
    test_zint(std::u64::MAX)?;
    Ok(())
}
