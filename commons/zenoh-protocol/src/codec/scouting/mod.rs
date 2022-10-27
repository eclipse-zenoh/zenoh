use crate::message::scouting::ScoutingMessage;

//
// Copyright (c) 2022 ZettaScale Technology
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
mod hello;
mod scout;

use crate::codec::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

// impl<W> WCodec<&mut W, &ScoutingMessage> for Zenoh060
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: &ScoutingMessage) -> Self::Output {
//         self.write(&mut *writer, x.header())?;
//         match x.what {
//             Some(w) => {
//                 let w: ZInt = w.into();
//                 self.write(&mut *writer, w)
//             }
//             None => Ok(()),
//         }
//     }
// }

// impl<'a, R> RCodec<&'a mut R, ScoutingMessage> for Zenoh060
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<ScoutingMessage, Self::Error> {
//         let codec = Zenoh060RCodec {
//             header: self.read(&mut *reader)?,
//             ..Default::default()
//         };
//         codec.read(reader)
//     }
// }
