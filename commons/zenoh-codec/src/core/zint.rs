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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};

const VLE_LEN_MAX: usize = vle_len(u64::MAX);

const fn vle_len(x: u64) -> usize {
    const B1: u64 = u64::MAX << 7;
    const B2: u64 = u64::MAX << (7 * 2);
    const B3: u64 = u64::MAX << (7 * 3);
    const B4: u64 = u64::MAX << (7 * 4);
    const B5: u64 = u64::MAX << (7 * 5);
    const B6: u64 = u64::MAX << (7 * 6);
    const B7: u64 = u64::MAX << (7 * 7);
    const B8: u64 = u64::MAX << (7 * 8);

    if (x & B1) == 0 {
        1
    } else if (x & B2) == 0 {
        2
    } else if (x & B3) == 0 {
        3
    } else if (x & B4) == 0 {
        4
    } else if (x & B5) == 0 {
        5
    } else if (x & B6) == 0 {
        6
    } else if (x & B7) == 0 {
        7
    } else if (x & B8) == 0 {
        8
    } else {
        9
    }
}

impl LCodec<u64> for Zenoh080 {
    fn w_len(self, x: u64) -> usize {
        vle_len(x)
    }
}

impl LCodec<usize> for Zenoh080 {
    fn w_len(self, x: usize) -> usize {
        self.w_len(x as u64)
    }
}

impl LCodec<u32> for Zenoh080 {
    fn w_len(self, x: u32) -> usize {
        self.w_len(x as u64)
    }
}

impl LCodec<u16> for Zenoh080 {
    fn w_len(self, x: u16) -> usize {
        self.w_len(x as u64)
    }
}

impl LCodec<u8> for Zenoh080 {
    fn w_len(self, _: u8) -> usize {
        1
    }
}

// u8
impl<W> WCodec<u8, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: u8) -> Self::Output {
        writer.write_u8(x)
    }
}

impl<R> RCodec<u8, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<u8, Self::Error> {
        reader.read_u8()
    }
}

// u64
impl<W> WCodec<u64, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, mut x: u64) -> Self::Output {
        let write = move |buffer: &mut [u8]| {
            let mut len = 0;
            while (x & !0x7f_u64) != 0 {
                // SAFETY: buffer is guaranteed to be VLE_LEN long where VLE_LEN is
                //         the maximum number of bytes a VLE can take once encoded.
                //         I.e.: x is shifted 7 bits to the right every iteration,
                //         the loop is at most VLE_LEN iterations.
                unsafe {
                    *buffer.get_unchecked_mut(len) = (x as u8) | 0x80_u8;
                }
                len += 1;
                x >>= 7;
            }
            // In case len == VLE_LEN then all the bits have already been written in the latest iteration.
            // Else we haven't written all the necessary bytes yet.
            if len != VLE_LEN_MAX {
                // SAFETY: buffer is guaranteed to be VLE_LEN long where VLE_LEN is
                //         the maximum number of bytes a VLE can take once encoded.
                //         I.e.: x is shifted 7 bits to the right every iteration,
                //         the loop is at most VLE_LEN iterations.
                unsafe {
                    *buffer.get_unchecked_mut(len) = x as u8;
                }
                len += 1;
            }
            // The number of written bytes
            len
        };
        // SAFETY: write algorithm guarantees than returned length is lesser than or equal to
        // `VLE_LEN_MAX`.
        unsafe { writer.with_slot(VLE_LEN_MAX, write)? };
        Ok(())
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
        // 7 * VLE_LEN is beyond the maximum number of shift bits
        while (b & 0x80_u8) != 0 && i != 7 * (VLE_LEN_MAX - 1) {
            v |= ((b & 0x7f_u8) as u64) << i;
            b = reader.read_u8()?;
            i += 7;
        }
        v |= (b as u64) << i;
        Ok(v)
    }
}

// Derive impls
macro_rules! uint_impl {
    ($uint:ty) => {
        impl<W> WCodec<$uint, &mut W> for Zenoh080
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: $uint) -> Self::Output {
                self.write(writer, x as u64)
            }
        }

        impl<R> RCodec<$uint, &mut R> for Zenoh080
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<$uint, Self::Error> {
                let x: u64 = self.read(reader)?;
                Ok(x as $uint)
            }
        }
    };
}

uint_impl!(u16);
uint_impl!(u32);
uint_impl!(usize);

macro_rules! uint_ref_impl {
    ($uint:ty) => {
        impl<W> WCodec<&$uint, &mut W> for Zenoh080
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: &$uint) -> Self::Output {
                self.write(writer, *x)
            }
        }
    };
}

uint_ref_impl!(u8);
uint_ref_impl!(u16);
uint_ref_impl!(u32);
uint_ref_impl!(u64);
uint_ref_impl!(usize);

// Encode unsigned integer and verify that the size boundaries are respected
macro_rules! zint_impl_codec {
    ($zint:ty, $bound:ty) => {
        impl<W> WCodec<$zint, &mut W> for Zenoh080Bounded<$bound>
        where
            W: Writer,
        {
            type Output = Result<(), DidntWrite>;

            fn write(self, writer: &mut W, x: $zint) -> Self::Output {
                if (x as u64 & !(<$bound>::MAX as u64)) != 0 {
                    return Err(DidntWrite);
                }
                Zenoh080.write(writer, x as u64)
            }
        }

        impl<R> RCodec<$zint, &mut R> for Zenoh080Bounded<$bound>
        where
            R: Reader,
        {
            type Error = DidntRead;

            fn read(self, reader: &mut R) -> Result<$zint, Self::Error> {
                let x: u64 = Zenoh080.read(reader)?;
                if (x & !(<$bound>::MAX as u64)) != 0 {
                    return Err(DidntRead);
                }
                Ok(x as $zint)
            }
        }
    };
}
macro_rules! zint_impl {
    ($zint:ty) => {
        zint_impl_codec!($zint, u8);
        zint_impl_codec!($zint, u16);
        zint_impl_codec!($zint, u32);
        zint_impl_codec!($zint, u64);
        zint_impl_codec!($zint, usize);
    };
}

zint_impl!(u8);
zint_impl!(u16);
zint_impl!(u32);
zint_impl!(u64);
zint_impl!(usize);

// const MAX_LEN: usize = 9;
// const VLE_THR: u64 = 0xf8; // 248
// impl<W> WCodec<u64, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;
//     fn write(self, writer: &mut W, mut x: u64) -> Self::Output {
//         writer.with_slot(MAX_LEN, move |into| {
//             if x < VLE_THR {
//                 into[0] = x as u8;
//                 return 1;
//             }
//             x -= VLE_THR - 1;
//             // SAFETY
//             // The `if x < VLE_THR` check at the beginning followed by `x -= VLE_THR - 1`
//             // guarantees at this point that `x` is never `0`. Since `x` is 64bit,
//             // then `n` is guaranteed to have a value between 1 and 8, both inclusives.
//             // `into` is guaranteed to be exactly 9 bytes long. Therefore, copying at most
//             // 8 bytes with a pointer offset of 1 is actually safe.
//             let n = 8 - (x.leading_zeros() / 8) as usize;
//             unsafe {
//                 core::ptr::copy_nonoverlapping(
//                     x.to_le_bytes().as_ptr(),
//                     into.as_mut_ptr().offset(1),
//                     n,
//                 )
//             }
//             into[0] = VLE_THR as u8 | (n - 1) as u8;
//             1 + n
//         })
//     }
// }

// impl<R> RCodec<u64, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;
//     fn read(self, reader: &mut R) -> Result<u64, Self::Error> {
//         let b = reader.read_u8()?;
//         if b < (VLE_THR as u8) {
//             return Ok(b as u64);
//         }
//         let n = (1 + (b & !VLE_THR as u8)) as usize;
//         let mut u64: [u8; 8] = 0u64.to_le_bytes();
//         reader.read_exact(&mut u64[0..n])?;
//         let u64 = u64::from_le_bytes(u64);
//         Ok(u64.saturating_add(VLE_THR - 1))
//     }
// }

// mod tests {
//     #[test]
//     fn u64_overhead() {
//         use crate::{WCodec, Zenoh080};
//         use zenoh_buffers::{
//             reader::{HasReader, Reader},
//             writer::HasWriter,
//         };

//         fn overhead(x: u64) -> usize {
//             let codec = Zenoh080::new();
//             let mut b = vec![];
//             let mut w = b.writer();
//             codec.write(&mut w, x).unwrap();
//             let r = b.reader().remaining();
//             println!("{} {}", x, r);
//             r
//         }

//         assert_eq!(overhead(247), 1);
//         assert_eq!(overhead(248), 2);
//         assert_eq!(overhead(502), 2);
//         assert_eq!(overhead(503), 3);
//         assert_eq!(overhead(65_782), 3);
//         assert_eq!(overhead(65_783), 4);
//         assert_eq!(overhead(16_777_462), 4);
//         assert_eq!(overhead(16_777_463), 5);
//         assert_eq!(overhead(4_294_967_542), 5);
//         assert_eq!(overhead(4_294_967_543), 6);
//         assert_eq!(overhead(1_099_511_628_022), 6);
//         assert_eq!(overhead(1_099_511_628_023), 7);
//         assert_eq!(overhead(281_474_976_710_902), 7);
//         assert_eq!(overhead(281_474_976_710_903), 8);
//         assert_eq!(overhead(72_057_594_037_928_182), 8);
//         assert_eq!(overhead(72_057_594_037_928_183), 9);
//         assert_eq!(overhead(u64::MAX), 9);
//     }
// }

// macro_rules! non_zero_array {
//     ($($i: expr,)*) => {
//         [$(match NonZeroU8::new($i) {Some(x) => x, None => panic!("Attempted to place 0 in an array of non-zeros literal")}),*]
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
//     fn u64_fuzz() {
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
