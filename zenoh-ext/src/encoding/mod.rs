//
// Copyright (c) 2024 ZettaScale Technology
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
pub mod iana;
pub mod serde;

use std::borrow::Cow;
use zenoh::encoding::{DefaultEncoding, Encoding, EncodingMapping, EncodingPrefix};
use zenoh_result::ZResult;

#[macro_export]
macro_rules! derive_default_encoding_for {
    ($e:ty) => {
        macro_rules! default_encoder {
            ($t:ty) => {
                impl zenoh::encoding::Encoder<$t> for $e {
                    type Output = <zenoh::encoding::DefaultEncoding as zenoh::encoding::Encoder<$t>>::Output;

                    fn encode(self, t: $t) -> Self::Output {
                        zenoh::encoding::DefaultEncoding.encode(t)
                    }
                }
            };
        }

        macro_rules! default_encoder_ref {
            ($t:ty) => {
                impl<'a> zenoh::encoding::Encoder<&'a $t> for $e {
                    type Output = <zenoh::encoding::DefaultEncoding as zenoh::encoding::Encoder<&'a $t>>::Output;

                    fn encode(self, t: &'a $t) -> Self::Output {
                        zenoh::encoding::DefaultEncoding.encode(t)
                    }
                }
            };
        }

        macro_rules! default_decoder {
            ($t:ty) => {
                // impl zenoh::encoding::Decoder<$t> for $e {
                //     type Error = <zenoh::encoding::DefaultEncoding as zenoh::encoding::Decoder<$t>>::Error;

                //     fn decode(self, v: &zenoh::value::Value) -> Result<$t, Self::Error> {
                //         zenoh::encoding::DefaultEncoding.decode(v)
                //     }
                // }
            };
        }

        macro_rules! default_both {
            ($t:ty) => {
                default_encoder!($t);
                default_decoder!($t);
            };
        }


        macro_rules! default_encoder_cow {
            ($t:ty) => {
                impl<'a> zenoh::encoding::Encoder<Cow<'a, $t>> for $e {
                    type Output = <zenoh::encoding::DefaultEncoding as zenoh::encoding::Encoder<Cow<'a, $t>>>::Output;

                    fn encode(self, t: Cow<'a, $t>) -> Self::Output {
                        zenoh::encoding::DefaultEncoding.encode(t)
                    }
                }
            };
        }

        macro_rules! default_decoder_cow {
            ($t:ty) => {
                // impl<'a> zenoh::encoding::Decoder<Cow<'a, $t>> for $e {
                //     type Error = <zenoh::encoding::DefaultEncoding as zenoh::encoding::Decoder<Cow<'a, $t>>>::Error;

                //     fn decode(self, v: &zenoh::value::Value) -> Result<Cow<'a, $t>, Self::Error> {
                //         zenoh::encoding::DefaultEncoding.decode(v)
                //     }
                // }
            };
        }

        macro_rules! default_both_cow {
            ($t:ty) => {
                default_encoder_cow!($t);
                default_decoder_cow!($t);
            };
        }

        // Octect stream
        default_both!(zenoh::buffers::ZBuf);
        default_both!(Vec<u8>);
        default_encoder_ref!([u8]);
        default_both_cow!([u8]);
        // Text plain
        default_both!(String);
        default_encoder_ref!(str);
        default_both_cow!(str);
        // Unsigned integer
        default_both!(u8);
        default_both!(u16);
        default_both!(u32);
        default_both!(u64);
        default_both!(usize);
        // Signed integer
        default_both!(i8);
        default_both!(i16);
        default_both!(i32);
        default_both!(i64);
        default_both!(isize);
        // Bool
        default_both!(bool);
        // Float
        default_both!(f32);
        default_both!(f64);
        // Sample
        default_encoder!(zenoh::prelude::Sample);
    };
}

/// A generic way to extend encoding mappings.
pub struct ExtendedEncodingMapping<With, Base = DefaultEncoding>
where
    With: EncodingMapping,
    Base: EncodingMapping,
{
    base: Base,
    with: With,
}

impl<With, Base> EncodingMapping for ExtendedEncodingMapping<With, Base>
where
    With: EncodingMapping,
    Base: EncodingMapping,
{
    const MIN: EncodingPrefix = Base::MIN;
    const MAX: EncodingPrefix = With::MAX;

    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> Option<Cow<'_, str>> {
        match self.base.prefix_to_str(p) {
            Some(s) => Some(s),
            None => self.with.prefix_to_str(p),
        }
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`IanaEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> Option<EncodingPrefix> {
        match self.base.str_to_prefix(s) {
            Some(p) => Some(p),
            None => self.with.str_to_prefix(s),
        }
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`IanaEncodingMapping::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse<T, U>(base: &U, with: &T, t: Cow<'static, str>) -> ZResult<Encoding>
        where
            T: EncodingMapping,
            U: EncodingMapping,
        {
            // Check if empty
            if t.is_empty() {
                return Ok(Encoding::empty());
            }
            // Try first an exact lookup of the string to prefix for the DefaultEncodingMapping
            if let Some(p) = base.str_to_prefix(t.as_ref()) {
                return Ok(Encoding::new(p));
            }
            // Try first an exact lookup of the string to prefix for the DefaultEncodingMapping
            if let Some(p) = with.str_to_prefix(t.as_ref()) {
                return Ok(Encoding::new(p));
            }
            // Find the most efficient mapping
            let d = base.parse(t.to_string())?;
            let e = with.parse(t)?;
            if d.suffix().len() < e.suffix().len() {
                Ok(d)
            } else {
                Ok(e)
            }
        }

        _parse(&self.base, &self.with, t.into())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str(&self, e: &Encoding) -> Cow<'_, str> {
        if (Base::MIN..=Base::MAX).contains(&e.prefix()) {
            self.base.to_str(e)
        } else {
            self.with.to_str(e)
        }
    }
}

// A generic way to extend encoding.

// pub struct ExtendedEncoding<With, Base = DefaultEncoding>
// where
//     With: ZEncoding,
//     Base: ZEncoding,
// {
//     base: Base,
//     with: With,
// }

// impl<T, With, Base> Encoder<T> for ExtendedEncoding<With, Base>
// where
//     With: ZEncoding,
//     Base: ZEncoding + Encoder<T>,
// {
//     type Output = <Base as Encoder<T>>::Output;

//     fn encode(self, t: T) -> Self::Output {
//         self.base.encode(t)
//     }
// }

// impl<T, With, Base> Encoder<T> for ExtendedEncoding<With, Base>
// where
//     With: ZEncoding + Encoder<T>,
//     Base: ZEncoding,
// {
//     type Output = <With as Encoder<T>>::Output;

//     fn encode(self, t: T) -> Self::Output {
//         self.with.encode(t)
//     }
// }

// impl<T> Decoder<T> for ExtendedEncoding<T>
// where
//     DefaultEncoding: Decoder<T>,
// {
//     type Error = <DefaultEncoding as Decoder<T>>::Error;

//     fn decode(self, v: &Value) -> Result<T, Self::Error> {
//         DefaultEncoding.decode(v)
//     }
// }
