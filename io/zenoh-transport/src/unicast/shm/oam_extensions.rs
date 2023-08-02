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

use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZBuf};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    common::ZExtBody,
    network::{
        oam::{
            self,
            id::{OAM_CLOSE, OAM_KEEPALIVE},
        },
        NetworkBody, NetworkMessage, Oam,
    },
    transport::Close,
};
use zenoh_result::{bail, zerror, ZResult};

pub(crate) fn pack_oam_close(close: Close) -> ZResult<NetworkMessage> {
    let codec = Zenoh080::new();
    let mut buf = ZBuf::empty();
    codec
        .write(&mut buf.writer(), &close)
        .map_err(|_| zerror!("Error serializing Close as Network OAM extension"))?;

    let msg = NetworkMessage {
        body: NetworkBody::OAM(Oam {
            id: OAM_CLOSE,
            body: ZExtBody::ZBuf(buf),
            ext_qos: oam::ext::QoSType::default(),
            ext_tstamp: None,
        }),
    };

    Ok(msg)
}

pub(crate) fn unpack_oam_close(oam: &Oam) -> ZResult<Close> {
    match oam.body {
        ZExtBody::ZBuf(ref body) => {
            let codec = Zenoh080::new();
            let mut reader = body.reader();
            let close: Close = codec
                .read(&mut reader)
                .map_err(|_| zerror!("{:?}: decoding error", oam))?;
            Ok(close)
        }
        _ => {
            bail!("{:?}: wrong Oam body type", oam)
        }
    }
}

pub(crate) fn pack_oam_keepalive() -> NetworkMessage {
    NetworkMessage {
        body: NetworkBody::OAM(Oam {
            id: OAM_KEEPALIVE,
            body: ZExtBody::ZBuf(ZBuf::default()),
            ext_qos: oam::ext::QoSType::default(),
            ext_tstamp: None,
        }),
    }
}
