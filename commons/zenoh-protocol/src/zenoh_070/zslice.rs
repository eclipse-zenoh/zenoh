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
use super::Zenoh070;
use crate::codec_traits::{RCodec, WCodec};
use std::num::NonZeroUsize;
use zenoh_buffers::{
    traits::{
        buffer::{Buffer, DidntWrite},
        reader::Reader,
    },
    ZSlice,
};

impl<B> WCodec<&mut B, ZSlice> for &Zenoh070
where
    B: Buffer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut B, x: ZSlice) -> Self::Output {
        self.write(writer, x.len())?;
        writer.append(x)?;
        Ok(())
    }
}

// fn read_from_socket(link: Link) {
//     loop {
//         let mut buff = vec![0u8; u16::MAX as usize];
//         link.read(&mut buff);

//         let zslice = ZSlice::from(buff);

//         while let Some(msg) = zslice.read_msg() {
//             // DO STUFF
//         }
//     }
// }

// struct Reader {
//     inner: ZSlice,
//     pos: usize,
// }

// impl<R> RCodec<&mut R, ZSlice> for &Zenoh070
// where
//     R: Reader,
// {
//     type Error = ();
//     fn read(self, reader: &mut R) -> Result<ZSlice, Self::Error> {
//         Err(())
//     }
// }
