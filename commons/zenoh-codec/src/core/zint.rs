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
use crate::{RCodec, WCodec, Zenoh060};
use core::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

const VLE_LEN: usize = 10;

// ZInt
impl<W> WCodec<u64, &mut W> for Zenoh060
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
        })?;
        Ok(())
    }
}

impl<W> WCodec<&u64, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &u64) -> Self::Output {
        self.write(writer, *x)
    }
}

impl<R> RCodec<u64, &mut R> for Zenoh060
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
impl<W> WCodec<usize, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: usize) -> Self::Output {
        let x: u64 = x.try_into().map_err(|_| DidntWrite)?;
        self.write(writer, x)
    }
}

impl<R> RCodec<usize, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<usize, Self::Error> {
        let x: u64 = <Self as RCodec<u64, &mut R>>::read(self, reader)?;
        x.try_into().map_err(|_| DidntRead)
    }
}

// macro_rules! non_zero_array {
//     ($($i: expr,)*) => {
//         [$(match NonZeroU8::new($i) {Some(x) => x, None => panic!("Attempted to place 0 in an array of non-zeros litteral")}),*]
//     };
// }

// impl Zenoh070 {
//     pub const fn preview_length(&self, x: u64) -> NonZeroU8 {
//         let x = match core::num::NonZeroU64::new(x) {
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

// macro_rules! impl_unsigned_codec {
//     ($t: ty) => {
//         impl<Codec, Buffer> WCodec<Buffer, $t> for Codec
//         where
//             Codec: WCodec<Buffer, u64>,
//         {
//             type Output = <Codec as WCodec<Buffer, u64>>::Output;
//             fn write(self, writer: Buffer, x: $t) -> Self::Output {
//                 self.write(writer, x as u64)
//             }
//         }

//         impl<Codec, Reader> RCodec<Reader, $t> for Codec
//         where
//             Codec: RCodec<Reader, u64>,
//         {
//             type Error = ConversionOrReadError<u64, $t, <Codec as RCodec<Reader, u64>>::Error>;

//             fn read(self, writer: Reader) -> Result<$t, Self::Error> {
//                 match self.read(writer) {
//                     Ok(v) => v
//                         .try_into()
//                         .map_err(|e| ConversionOrReadError::ConversionError(e)),
//                     Err(e) => Err(ConversionOrReadError::ReadError(e)),
//                 }
//             }
//         }
//     };
// }

// impl_unsigned_codec!(usize);
// impl_unsigned_codec!(u32);
// impl_unsigned_codec!(u16);
// impl_unsigned_codec!(u8);

// #[derive(Debug, Clone)]
// pub enum ConversionOrReadError<T, U, E>
// where
//     T: TryInto<U>,
//     <T as TryInto<U>>::Error: core::fmt::Debug + Clone,
// {
//     ReadError(E),
//     ConversionError(<T as TryInto<U>>::Error),
// }

// impl<W> WCodec<&mut W, u64> for &Zenoh070
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: u64) -> Self::Output {
//         const VLE_SHIFT: [u8; 65] = [
//             1, // This is padding to avoid needless subtractions on index access
//             1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 5,
//             5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 0, 0,
//             0, 0, 0, 0, 0, 0,
//         ];
//         const VLE_MASK: [u8; 9] = [
//             0b11111111, // This is padding to avoid needless subtractions on index access
//             0, 0b00000001, 0b00000011, 0b00000111, 0b00001111, 0b00011111, 0b00111111, 0b01111111,
//         ];
//         const VLE_LEN: usize = 9;
//         writer.with_slot(VLE_LEN, move |mut buffer| {
//             // since leading_zeros will jump conditionally on 0 anyway (`asm {bsr 0}` is UB), might as well jump to return
//             let x = match core::num::NonZeroU64::new(x) {
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
//                 core::ptr::copy_nonoverlapping(
//                     serialized.as_ptr(),
//                     buffer.as_mut_ptr().offset(shift_payload as isize),
//                     u64::BITS as usize / 8,
//                 )
//             };
//             let needed_bytes = payload_size as usize;
//             buffer[0] |= VLE_MASK[needed_bytes];
//             let to_write = if shift_payload { VLE_LEN } else { needed_bytes };
//             to_write
//         })?;
//         Ok(())
//     }
// }
// impl<'a, R> RCodec<&mut R, u64> for &Zenoh070
// where
//     R: Reader<'a>,
// {
//     type Error = ();
//     fn read(self, reader: &mut R) -> Result<u64, Self::Error> {
//         let mut buffer = [0; 8];
//         buffer[0] = match reader.read_u8() {
//             None => return Err(()),
//             Some(x) => x,
//         };
//         // GCC: `__builtin_ctz(~buffer[0])`, clang: `__tzcnt_u64(~buffer[0])`
//         let byte_count = (buffer[0].trailing_ones()) as usize;
//         if byte_count == 0 {
//             return Ok(u64::from_le_bytes(buffer) >> 1);
//         }
//         let shift_payload = (byte_count == 8) as usize; // branches are evil
//         let shift_payload_multiplier = (1 - shift_payload) as usize;

//         let len = byte_count + shift_payload_multiplier;
//         reader.read(&mut buffer[shift_payload_multiplier..len]);

//         // the shift also removes the mask
//         Ok(u64::from_le_bytes(buffer) >> ((byte_count + 1) * shift_payload_multiplier) as u32)
//     }
// }

// #[cfg(test)]
// mod test {
//     #[test]
//     fn zint_fuzz() {
//         use crate::codec::*;
//         use rand::Rng;
//         let codec = crate::zenoh_070::Zenoh070::default();
//         let mut rng = rand::thread_rng();
//         let dist = rand::distributions::Uniform::new(0, u8::MAX as u64);
//         let mut buffer = Vec::with_capacity(9);
//         for _ in 0..1000000 {
//             buffer.clear();
//             let mut x: u64 = 1;
//             for _ in 0..(rng.sample(&dist) % 8) {
//                 x *= rng.sample(&dist);
//             }

//             (&codec).write(&mut buffer, x).unwrap();
//             let mut reader = buffer.as_slice();
//             let y: u64 = (&codec).read(&mut reader).unwrap();
//             assert!(reader.is_empty());
//             assert_eq!(
//                 x,
//                 y,
//                 "\n         {} ({}) != \n         {} ({})\n{} ({})",
//                 binstr(&x.to_le_bytes()),
//                 x,
//                 binstr(&y.to_le_bytes()),
//                 y,
//                 binstr(&buffer),
//                 y,
//             );
//         }
//         buffer.clear();
//         let mut expected = Vec::new();
//         for _ in 0..1000 {
//             let mut x: u64 = 1;
//             for _ in 0..(rng.sample(&dist) % 8) {
//                 x *= rng.sample(&dist);
//             }
//             expected.push(x);
//             let _ = (&codec).write(&mut buffer, x);
//         }
//         println!("{:x?}", expected);
//         let mut reader = buffer.as_slice();
//         for (i, &x) in expected.iter().enumerate() {
//             let y: u64 = (&codec).read(&mut reader).unwrap();
//             assert_eq!(
//                 x,
//                 y,
//                 "after reading {} numbers\n         {} ({}) != \n         {} ({})\n{} ({})",
//                 i,
//                 binstr(&x.to_le_bytes()),
//                 x,
//                 binstr(&y.to_le_bytes()),
//                 y,
//                 binstr(&buffer),
//                 y,
//             );
//         }
//         assert!(reader.is_empty());
//     }

//     pub fn binstr(buffer: &[u8]) -> String {
//         let mut result = String::with_capacity(9 * buffer.len());
//         for byte in buffer {
//             for i in 0..8 {
//                 result.push(if (byte >> i) & 1 == 1 { '1' } else { '0' })
//             }
//             result.push(' ')
//         }
//         result
//     }
// }
