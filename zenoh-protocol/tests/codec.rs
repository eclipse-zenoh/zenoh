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
use zenoh_protocol::core::{ZInt, ZINT_MAX_BYTES};
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
    for i in 1..ZINT_MAX_BYTES {
        let res = (1 as ZInt).checked_shl(7*i as u32);
        if let Some(v) = res {
            test_zint(v-1)?;
            test_zint(v)?;
        } else {
            break;
        }        
    }
    test_zint(ZInt::MAX)?;
    Ok(())
}
