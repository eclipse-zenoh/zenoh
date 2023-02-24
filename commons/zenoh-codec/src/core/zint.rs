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
use crate::{RCodec, WCodec, Zenoh080};
use core::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

const VLE_LEN: usize = 10;

// ZInt
impl<W> WCodec<u64, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, mut x: u64) -> Self::Output {
        writer.with_slot(VLE_LEN, move |buffer| {
            let mut len = 0;
            let mut b = x as u8;
            while x > 0x7f {
                buffer[len] = b | 0x80;
                len += 1;
                x >>= 7;
                b = x as u8;
            }
            buffer[len] = b;
            len + 1
        })
    }
}

impl<W> WCodec<&u64, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &u64) -> Self::Output {
        self.write(writer, *x)
    }
}

impl<R> RCodec<u64, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<u64, Self::Error> {
        let mut b = reader.read_u8()?;

        let mut v = 0;
        let mut i = 0;
        let mut k = VLE_LEN;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            i += 7;
            b = reader.read_u8()?;
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            Ok(v)
        } else {
            Err(DidntRead)
        }
    }
}

// usize
impl<W> WCodec<usize, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: usize) -> Self::Output {
        let x: u64 = x.try_into().map_err(|_| DidntWrite)?;
        self.write(writer, x)
    }
}

impl<R> RCodec<usize, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<usize, Self::Error> {
        let x: u64 = self.read(reader)?;
        x.try_into().map_err(|_| DidntRead)
    }
}

// macro_rules! non_zero_array {
//     ($($i: expr,)*) => {
//         [$(match NonZeroU8::new($i) {Some(x) => x, None => panic!("Attempted to place 0 in an array of non-zeros litteral")}),*]
//     };
// }

// impl Zenoh080 {
//     pub const fn preview_length(&self, x: u64) -> NonZeroU8 {
//         let x = match NonZeroU64::new(x) {
//             Some(x) => x,
//             None => {
//                 return unsafe { NonZeroU8::new_unchecked(1) };
//             }
//         };
//         let needed_bits = u64::BITS - x.leading_zeros();
//         const LENGTH: [NonZeroU8; 65] = non_zero_array![
//             1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4,
//             5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 9,
//             9, 9, 9, 9, 9, 9, 9,
//         ];
//         LENGTH[needed_bits as usize]
//     }
// }

// impl<W> WCodec<u64, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: u64) -> Self::Output {
//         const VLE_LEN: usize = 9;
//         const VLE_MASK: [u8; VLE_LEN] = [
//             0b11111111, // This is padding to avoid needless subtractions on index access
//             0, 0b00000001, 0b00000011, 0b00000111, 0b00001111, 0b00011111, 0b00111111, 0b01111111,
//         ];
//         const VLE_SHIFT: [u8; 65] = [
//             1, // This is padding to avoid needless subtractions on index access
//             1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 5,
//             5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 0, 0,
//             0, 0, 0, 0, 0, 0,
//         ];

//         writer.with_slot(VLE_LEN, move |buffer| {
//             // since leading_zeros will jump conditionally on 0 anyway (`asm {bsr 0}` is UB), might as well jump to return
//             let x = match NonZeroU64::new(x) {
//                 Some(x) => x,
//                 None => {
//                     buffer[0] = 0;
//                     return 1;
//                 }
//             };

//             let needed_bits = u64::BITS - x.leading_zeros();
//             let payload_size = VLE_SHIFT[needed_bits as usize];
//             let shift_payload = payload_size == 0;
//             let mut x: u64 = x.into();
//             x <<= payload_size;
//             let serialized = x.to_le_bytes();

//             unsafe {
//                 ptr::copy_nonoverlapping(
//                     serialized.as_ptr(),
//                     buffer.as_mut_ptr().offset(shift_payload as isize),
//                     u64::BITS as usize / 8,
//                 )
//             };

//             let needed_bytes = payload_size as usize;
//             buffer[0] |= VLE_MASK[needed_bytes];
//             if shift_payload {
//                 VLE_LEN
//             } else {
//                 needed_bytes
//             }
//         })?;

//         Ok(())
//     }
// }

// impl<W> WCodec<&u64, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: &u64) -> Self::Output {
//         self.write(writer, *x)
//     }
// }

// impl<R> RCodec<u64, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<u64, Self::Error> {
//         let mut buffer = [0; 8];
//         buffer[0] = reader.read_u8()?;

//         // GCC: `__builtin_ctz(~buffer[0])`, clang: `__tzcnt_u64(~buffer[0])`
//         let byte_count = (buffer[0].trailing_ones()) as usize;
//         if byte_count == 0 {
//             return Ok(u64::from_le_bytes(buffer) >> 1);
//         }

//         let shift_payload = (byte_count == 8) as usize; // branches are evil
//         let shift_payload_multiplier = 1 - shift_payload;

//         let len = byte_count + shift_payload_multiplier;
//         reader.read_exact(&mut buffer[shift_payload_multiplier..len])?;

//         // the shift also removes the mask
//         Ok(u64::from_le_bytes(buffer) >> ((byte_count + 1) * shift_payload_multiplier) as u32)
//     }
// }

// // usize
// impl<W> WCodec<usize, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: usize) -> Self::Output {
//         let x: u64 = x.try_into().map_err(|_| DidntWrite)?;
//         self.write(writer, x)
//     }
// }

// impl<R> RCodec<usize, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<usize, Self::Error> {
//         let x: u64 = <Self as RCodec<u64, &mut R>>::read(self, reader)?;
//         x.try_into().map_err(|_| DidntRead)
//     }
// }

// #[cfg(test)]
// mod test {
//     #[test]
//     fn zint_fuzz() {
//         use crate::*;
//         use rand::Rng;
//         use zenoh_buffers::{reader::HasReader, writer::HasWriter};

//         const NUM: usize = 1_000;
//         const LIMIT: [u64; 4] = [u8::MAX as u64, u16::MAX as u64, u32::MAX as u64, u64::MAX];

//         let codec = Zenoh080::new();
//         let mut rng = rand::thread_rng();

//         for l in LIMIT.iter() {
//             let mut values = Vec::with_capacity(NUM);
//             let mut buffer = vec![];

//             let mut writer = buffer.writer();

//             for _ in 0..NUM {
//                 let x: u64 = rng.gen_range(0..=*l);
//                 values.push(x);
//                 codec.write(&mut writer, x).unwrap();
//             }

//             let mut reader = buffer.reader();

//             for x in values.drain(..).take(NUM) {
//                 let y: u64 = codec.read(&mut reader).unwrap();
//                 println!("{x} {y}");
//                 assert_eq!(x, y);
//             }

//             assert!(reader.is_empty());
//         }
//     }
// }
