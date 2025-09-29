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
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::Hash,
    io::{Read, Write},
    marker::PhantomData,
    mem::MaybeUninit,
};

use zenoh::bytes::{ZBytes, ZBytesReader, ZBytesWriter};

/// Error occurring in deserialization.
#[derive(Debug)]
pub struct ZDeserializeError;
impl fmt::Display for ZDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "deserialization error")
    }
}
impl std::error::Error for ZDeserializeError {}

fn default_serialize_n<T: Serialize>(slice: &[T], serializer: &mut ZSerializer) {
    for t in slice {
        t.serialize(serializer)
    }
}

/// Serialization implementation.
///
/// See [Zenoh serialization format RFC][1].
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
pub trait Serialize {
    /// Serialize the given object into a [`ZSerializer`].
    ///
    /// User may prefer to use [`ZSerializer::serialize`] instead of this function.
    fn serialize(&self, serializer: &mut ZSerializer);
    #[doc(hidden)]
    fn serialize_n(slice: &[Self], serializer: &mut ZSerializer)
    where
        Self: Sized,
    {
        default_serialize_n(slice, serializer);
    }
}
impl<T: Serialize + ?Sized> Serialize for &T {
    fn serialize(&self, serializer: &mut ZSerializer) {
        T::serialize(*self, serializer)
    }
}

fn default_deserialize_n<T: Deserialize>(
    in_place: &mut [T],
    deserializer: &mut ZDeserializer,
) -> Result<(), ZDeserializeError> {
    for t in in_place {
        *t = T::deserialize(deserializer)?;
    }
    Ok(())
}

fn default_deserialize_n_uninit<'a, T: Deserialize>(
    in_place: &'a mut [MaybeUninit<T>],
    deserializer: &mut ZDeserializer,
) -> Result<&'a mut [T], ZDeserializeError> {
    for t in in_place.iter_mut() {
        t.write(T::deserialize(deserializer)?);
    }
    // SAFETY: all members of the slices have been initialized
    Ok(unsafe { &mut *(in_place as *mut [MaybeUninit<T>] as *mut [T]) })
}

/// Deserialization implementation.
///
/// See [Zenoh serialization format RFC][1].
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
pub trait Deserialize: Sized {
    /// Deserialize the given type from a [`ZDeserializer`].
    ///
    /// User may prefer to use [`ZDeserializer::deserialize`] instead of this function.
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError>;
    #[doc(hidden)]
    fn deserialize_n(
        in_place: &mut [Self],
        deserializer: &mut ZDeserializer,
    ) -> Result<(), ZDeserializeError> {
        default_deserialize_n(in_place, deserializer)
    }
    #[doc(hidden)]
    fn deserialize_n_uninit<'a>(
        in_place: &'a mut [MaybeUninit<Self>],
        deserializer: &mut ZDeserializer,
    ) -> Result<&'a mut [Self], ZDeserializeError> {
        default_deserialize_n_uninit(in_place, deserializer)
    }
}

/// Serialize an object according to the [Zenoh serialization format][1].
///
/// Serialization doesn't take the ownership of the data.
///
/// This function takes any type implementing the [`Serialize`] trait and
/// creates [`ZBytes`] containing the serialized data.
/// This trait is implemented for all the primitive types (integers, floats, bool) and
/// standard collections (arrays, slices, `Vec`, `String`, `HashMap`, `HashSet`, etc)
/// and could be implemented for user-defined types.
///
/// # Examples
///
/// ```rust
/// use zenoh_ext::*;
/// let zbytes = z_serialize(&(42i32, vec![1u8, 2, 3]));
/// assert_eq!(z_deserialize::<(i32, Vec<u8>)>(&zbytes).unwrap(), (42i32, vec![1u8, 2, 3]));
/// ```
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
pub fn z_serialize<T: Serialize + ?Sized>(t: &T) -> ZBytes {
    let mut serializer = ZSerializer::new();
    serializer.serialize(t);
    serializer.finish()
}

/// Deserialize an object according to the [Zenoh serialization format][1].
///
/// The destination type is given as a type parameter. If the data cannot be
/// deserialized into the given type, an error is returned.
///
/// # Examples
///
/// ```rust
/// use zenoh_ext::*;
/// let zbytes = z_serialize(&(42i32, vec![1u8, 2, 3]));
/// assert_eq!(z_deserialize::<(i32, Vec<u8>)>(&zbytes).unwrap(), (42i32, vec![1u8, 2, 3]));
/// ```
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
pub fn z_deserialize<T: Deserialize>(zbytes: &ZBytes) -> Result<T, ZDeserializeError> {
    let mut deserializer = ZDeserializer::new(zbytes);
    let t = T::deserialize(&mut deserializer)?;
    if !deserializer.done() {
        return Err(ZDeserializeError);
    }
    Ok(t)
}

/// Serializer implementing the [Zenoh serialization format][1].
///
/// Serializing objects one after the other is equivalent to serialize a tuple of these objects.
///
/// # Examples
///
/// ```rust
/// use zenoh_ext::*;
/// let mut serializer = ZSerializer::new();
/// serializer.serialize(42i32);
/// serializer.serialize(vec![1u8, 2, 3]);
/// let zbytes = serializer.finish();
/// assert_eq!(z_deserialize::<(i32, Vec<u8>)>(&zbytes).unwrap(), (42i32, vec![1u8, 2, 3]));
/// ```
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
#[derive(Debug)]
pub struct ZSerializer(ZBytesWriter);

impl ZSerializer {
    /// Instantiate a [`ZSerializer`].
    pub fn new() -> Self {
        Self(ZBytes::writer())
    }

    /// Serialize the given object into a [`ZSerializer`].
    ///
    /// Serialization doesn't take the ownership of the data.
    pub fn serialize<T: Serialize>(&mut self, t: T) {
        t.serialize(self)
    }

    /// Serialize the given iterator into a [`ZSerializer`].
    ///
    /// Sequence serialized with this method may be deserialized with [`ZDeserializer::deserialize_iter`].
    /// See [Zenoh serialization format RFC][1].
    ///
    /// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md#sequences
    pub fn serialize_iter<T: Serialize, I: IntoIterator<Item = T>>(&mut self, iter: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        let iter = iter.into_iter();
        self.serialize(VarInt(iter.len()));
        for t in iter {
            t.serialize(self);
        }
    }

    /// Finish serialization by returning a [`ZBytes`].
    pub fn finish(self) -> ZBytes {
        self.0.finish()
    }
}

impl Default for ZSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ZSerializer> for ZBytes {
    fn from(value: ZSerializer) -> Self {
        value.finish()
    }
}

/// Deserializer implementing the [Zenoh serialization format][1].
///
/// Deserializing objects one after the other is equivalent to serialize a tuple of these objects.
///
/// # Examples
///
/// ```rust
/// use zenoh_ext::*;
/// let zbytes = z_serialize(&(42i32, vec![1u8, 2, 3]));
/// let mut deserializer = ZDeserializer::new(&zbytes);
/// assert_eq!(deserializer.deserialize::<i32>().unwrap(), 42i32);
/// assert_eq!(deserializer.deserialize::<Vec<u8>>().unwrap(), vec![1u8, 2, 3]);
/// assert!(deserializer.done())
/// ```
///
/// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md
#[derive(Debug)]
pub struct ZDeserializer<'a>(ZBytesReader<'a>);

impl<'a> ZDeserializer<'a> {
    /// Instantiate a [`ZDeserializer`] from a [`ZBytes`].
    pub fn new(zbytes: &'a ZBytes) -> Self {
        Self(zbytes.reader())
    }

    /// Return true if there is no data left to deserialize.
    pub fn done(&self) -> bool {
        self.0.is_empty()
    }

    /// Deserialize the given type from a [`ZDeserializer`].
    pub fn deserialize<T: Deserialize>(&mut self) -> Result<T, ZDeserializeError> {
        T::deserialize(self)
    }

    /// Deserialize an iterator into a [`ZDeserializer`].
    ///
    /// Sequence deserialized with this method may have been serialized with [`ZSerializer::serialize_iter`].
    /// See [Zenoh serialization format RFC][1].
    ///
    /// [1]: https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md#sequences
    pub fn deserialize_iter<'b, T: Deserialize>(
        &'b mut self,
    ) -> Result<ZReadIter<'a, 'b, T>, ZDeserializeError> {
        let len = <VarInt<usize>>::deserialize(self)?.0;
        Ok(ZReadIter {
            deserializer: self,
            len,
            _phantom: PhantomData,
        })
    }
}

/// Iterator returned by [`ZDeserializer::deserialize_iter`].
pub struct ZReadIter<'a, 'b, T: Deserialize> {
    deserializer: &'b mut ZDeserializer<'a>,
    len: usize,
    _phantom: PhantomData<T>,
}

impl<T: Deserialize> Iterator for ZReadIter<'_, '_, T> {
    type Item = Result<T, ZDeserializeError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(T::deserialize(self.deserializer))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T: Deserialize> ExactSizeIterator for ZReadIter<'_, '_, T> {}

impl<T: Deserialize> Drop for ZReadIter<'_, '_, T> {
    fn drop(&mut self) {
        self.by_ref().for_each(drop);
    }
}

impl Serialize for ZBytes {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serializer.serialize(VarInt(self.len()));
        serializer.0.append(self.clone());
    }
}

macro_rules! impl_num {
    ($($ty:ty),* $(,)?) => {$(
        impl Serialize for $ty {
            fn serialize(&self, serializer: &mut ZSerializer) {
                serializer.0.write_all(&(*self).to_le_bytes()).unwrap();
            }
            fn serialize_n(slice: &[Self], serializer: &mut ZSerializer) where Self: Sized {
                if cfg!(target_endian = "little") || std::mem::size_of::<Self>() == 1 {
                    // SAFETY: transmuting numeric types to their little endian bytes is safe
                    serializer.0.write_all(unsafe { slice.align_to().1 }).unwrap();
                } else {
                    default_serialize_n(slice, serializer)
                }
            }
        }
        impl Deserialize for $ty {
            fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
                let mut buf = [0; { std::mem::size_of::<Self>() }];
                deserializer.0.read_exact(&mut buf).or(Err(ZDeserializeError))?;
                Ok(<$ty>::from_le_bytes(buf))
            }
            fn deserialize_n(in_place: &mut [Self], deserializer: &mut ZDeserializer) -> Result<(), ZDeserializeError> {
                let size = std::mem::size_of::<Self>();
                if cfg!(target_endian = "little") || size == 1 {
                    // SAFETY: transmuting numeric types to their little endian bytes is safe
                    let buf = unsafe {in_place.align_to_mut().1};
                    deserializer.0.read_exact(buf).or(Err(ZDeserializeError))?;
                    Ok(())
                } else {
                    default_deserialize_n(in_place, deserializer)
                }
            }
            fn deserialize_n_uninit<'a>(in_place: &'a mut [MaybeUninit<Self>], deserializer: &mut ZDeserializer) -> Result<&'a mut [Self], ZDeserializeError> {
                if cfg!(target_endian = "little") ||  std::mem::size_of::<Self>() == 1 {
                    // need to initialize the slice because of std::io::Read interface
                    in_place.fill(MaybeUninit::new(Self::default()));
                    // SAFETY: all members of the slices have been initialized
                    let initialized = unsafe { &mut *(in_place as *mut [MaybeUninit<Self>] as *mut [Self]) };
                    Self::deserialize_n(initialized, deserializer)?;
                    Ok(initialized)
                } else {
                    default_deserialize_n_uninit(in_place, deserializer)
                }
            }
        }
    )*};
}
impl_num!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);

impl Serialize for bool {
    fn serialize(&self, serializer: &mut ZSerializer) {
        (*self as u8).serialize(serializer);
    }
}
impl Deserialize for bool {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        match u8::deserialize(deserializer)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ZDeserializeError),
        }
    }
}

fn serialize_slice<T: Serialize>(slice: &[T], serializer: &mut ZSerializer) {
    serializer.serialize(VarInt(slice.len()));
    T::serialize_n(slice, serializer);
}

fn deserialize_slice<T: Deserialize>(
    deserializer: &mut ZDeserializer,
) -> Result<Box<[T]>, ZDeserializeError> {
    let len = <VarInt<usize>>::deserialize(deserializer)?.0;
    let mut vec = Vec::with_capacity(len);
    let slice = T::deserialize_n_uninit(&mut vec.spare_capacity_mut()[..len], deserializer)?;
    let (slice_ptr, slice_len) = (slice.as_ptr(), slice.len());
    assert_eq!((slice_ptr, slice_len), (vec.as_ptr(), len));
    // SAFETY: assertion checks the returned slice is vector's one, and it's returned initialized
    unsafe { vec.set_len(len) };
    Ok(vec.into_boxed_slice())
}

impl<T: Serialize> Serialize for [T] {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serialize_slice(self, serializer);
    }
}
impl<T: Serialize, const N: usize> Serialize for [T; N] {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serialize_slice(self.as_slice(), serializer);
    }
}
impl<'a, T: Serialize + 'a> Serialize for Cow<'a, [T]>
where
    [T]: ToOwned,
{
    fn serialize(&self, serializer: &mut ZSerializer) {
        serialize_slice(self, serializer);
    }
}
impl<T: Serialize> Serialize for Box<[T]> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serialize_slice(self, serializer);
    }
}
impl<T: Deserialize> Deserialize for Box<[T]> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        deserialize_slice(deserializer)
    }
}
impl<T: Serialize> Serialize for Vec<T> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serialize_slice(self, serializer)
    }
}
impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        if <VarInt<usize>>::deserialize(deserializer)?.0 != N {
            return Err(ZDeserializeError);
        }
        let mut array = std::array::from_fn(|_| MaybeUninit::uninit());
        let slice = T::deserialize_n_uninit(&mut array, deserializer)?;
        let (slice_ptr, slice_len) = (slice.as_ptr(), slice.len());
        assert_eq!((slice_ptr, slice_len), (array.as_ptr().cast::<T>(), N));
        // SAFETY: assertion checks the returned slice is array's one, and it's returned initialized
        Ok(array.map(|t| unsafe { t.assume_init() }))
    }
}
impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        Ok(deserialize_slice(deserializer)?.into_vec())
    }
}
impl<T: Serialize + Eq + Hash> Serialize for HashSet<T> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serializer.serialize_iter(self);
    }
}
impl<T: Deserialize + Eq + Hash> Deserialize for HashSet<T> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        deserializer.deserialize_iter()?.collect()
    }
}
impl<T: Serialize + Ord> Serialize for BTreeSet<T> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serializer.serialize_iter(self);
    }
}
impl<T: Deserialize + Ord> Deserialize for BTreeSet<T> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        deserializer.deserialize_iter()?.collect()
    }
}
impl<K: Serialize + Eq + Hash, V: Serialize> Serialize for HashMap<K, V> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serializer.serialize_iter(self);
    }
}
impl<K: Deserialize + Eq + Hash, V: Deserialize> Deserialize for HashMap<K, V> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        deserializer.deserialize_iter()?.collect()
    }
}
impl<K: Serialize + Ord, V: Serialize> Serialize for BTreeMap<K, V> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        serializer.serialize_iter(self);
    }
}
impl<K: Deserialize + Ord, V: Deserialize> Deserialize for BTreeMap<K, V> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        deserializer.deserialize_iter()?.collect()
    }
}
impl Serialize for str {
    fn serialize(&self, serializer: &mut ZSerializer) {
        self.as_bytes().serialize(serializer);
    }
}
impl Serialize for Cow<'_, str> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        self.as_bytes().serialize(serializer);
    }
}
impl Serialize for String {
    fn serialize(&self, serializer: &mut ZSerializer) {
        self.as_bytes().serialize(serializer);
    }
}
impl Deserialize for String {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        String::from_utf8(Deserialize::deserialize(deserializer)?).or(Err(ZDeserializeError))
    }
}

macro_rules! impl_tuple {
    ($($ty:ident/$i:tt),* $(,)?) => {
        impl_tuple!(@;$($ty/$i),*);
    };
    (@$($ty:ident/$i:tt),*; $next:ident/$next_i:tt $(, $remain:ident/$remain_i:tt)*) => {
        impl_tuple!(@@$($ty/$i),*);
        impl_tuple!(@$($ty/$i,)* $next/$next_i; $($remain/$remain_i),*);
    };
    (@$($ty:ident/$i:tt),*;) => {
        impl_tuple!(@@$($ty/$i),*);
    };
    (@@$($ty:ident/$i:tt),* $(,)?) => {
        #[allow(unused)]
        impl<$($ty: Serialize),*> Serialize for ($($ty,)*) {
            fn serialize(&self, serializer: &mut ZSerializer) {
                $(self.$i.serialize(serializer);)*
            }
        }
        #[allow(unused)]
        impl<$($ty: Deserialize),*> Deserialize for ($($ty,)*) {
            fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
                Ok(($($ty::deserialize(deserializer)?,)*))
            }
        }
    };
}
impl_tuple!(
    T0 / 0,
    T1 / 1,
    T2 / 2,
    T3 / 3,
    T4 / 4,
    T5 / 5,
    T6 / 6,
    T7 / 7,
    T8 / 8,
    T9 / 9,
    T10 / 10,
    T11 / 11,
    T12 / 12,
    T13 / 13,
    T14 / 14,
    T15 / 15,
);

/// The [LEB128](https://en.wikipedia.org/wiki/LEB128) variable-length integer encoding.
///
/// The `zenoh-ext` implements serialization and deserialization of integers in variable-length `LEB128` format.
/// Currently it's implemented for `usize` only using the wrapper struct `VarInt<usize>`:
/// the traits [`Serialize`] and [`Deserialize`] are implemented for `VarInt<usize>` using
/// [leb128](https://crates.io/crates/leb128) crate.
///
/// This is used internally for serializing lengths of sequences (slices, `Vec`, `String`, `HashMap`, etc)
/// and could be used by users to serialize integer values in a more compact way.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarInt<T>(pub T);
impl Serialize for VarInt<usize> {
    fn serialize(&self, serializer: &mut ZSerializer) {
        leb128::write::unsigned(&mut serializer.0, self.0 as u64).unwrap();
    }
}
impl Deserialize for VarInt<usize> {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        let n = leb128::read::unsigned(&mut deserializer.0).or(Err(ZDeserializeError))?;
        Ok(VarInt(<usize>::try_from(n).or(Err(ZDeserializeError))?))
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use rand::{thread_rng, Rng};

    use super::*;

    macro_rules! serialize_deserialize {
        ($ty:ty, $expr:expr) => {
            let expr: &$ty = &$expr;
            let payload = z_serialize(expr);
            let output = z_deserialize::<$ty>(&payload).unwrap();
            assert_eq!(*expr, output);
        };
    }

    const RANDOM_TESTS: Range<usize> = 0..1_000;

    #[test]
    fn numeric_serialization() {
        macro_rules! test_int {
            ($($ty:ty),* $(,)?) => {$(
                serialize_deserialize!($ty, <$ty>::MIN);
                serialize_deserialize!($ty, <$ty>::MAX);
                let mut rng = thread_rng();
                for _ in RANDOM_TESTS {
                    serialize_deserialize!($ty, rng.gen::<$ty>());
                }
            )*};
        }
        test_int!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);
    }

    #[test]
    fn varint_serialization() {
        serialize_deserialize!(VarInt<usize>, VarInt(<usize>::MIN));
        serialize_deserialize!(VarInt<usize>, VarInt(<usize>::MAX));
        let mut rng = thread_rng();
        for _ in RANDOM_TESTS {
            serialize_deserialize!(VarInt<usize>, VarInt(rng.gen::<usize>()));
        }
    }

    #[test]
    fn primitive_slice_serialization() {
        let vec = vec![42.0f64, 0.15];
        serialize_deserialize!(Vec<f64>, vec);
        let payload = z_serialize(vec.as_slice());
        assert_eq!(vec, z_deserialize::<Vec<f64>>(&payload).unwrap())
    }

    #[test]
    fn slice_serialization() {
        let vec = vec!["abc".to_string(), "def".to_string()];
        serialize_deserialize!(Vec<String>, vec);
        let payload = z_serialize(vec.as_slice());
        assert_eq!(vec, z_deserialize::<Vec<String>>(&payload).unwrap())
    }

    #[test]
    fn string_serialization() {
        let s = "serialization".to_string();
        serialize_deserialize!(String, s);
        let payload = z_serialize(s.as_str());
        assert_eq!(s, z_deserialize::<String>(&payload).unwrap())
    }

    #[test]
    fn tuple_serialization() {
        serialize_deserialize!(
            (VarInt<usize>, f32, String),
            (VarInt(42), 42.0f32, "42".to_string())
        );
    }

    #[test]
    fn hashmap_serialization() {
        let mut map = HashMap::new();
        map.insert("hello".to_string(), "world".to_string());
        serialize_deserialize!(HashMap<String, String>, map);
    }

    macro_rules! check_binary_format {
        ($expr:expr, $out:expr) => {
            let payload = z_serialize(&$expr);
            assert_eq!(payload.to_bytes(), $out);
        };
    }

    #[test]
    fn binary_format() {
        let i1: i32 = 1234566;
        check_binary_format!(i1, vec![134, 214, 18, 0]);
        let i2: i32 = -49245;
        check_binary_format!(i2, vec![163, 63, 255, 255]);
        let s: &str = "test";
        check_binary_format!(s, vec![4, 116, 101, 115, 116]);
        let t: (u16, f32, &str) = (500, 1234.0, "test");
        check_binary_format!(t, vec![244, 1, 0, 64, 154, 68, 4, 116, 101, 115, 116]);
        let v: Vec<i64> = vec![-100, 500, 100000, -20000000];
        check_binary_format!(
            v,
            vec![
                4, 156, 255, 255, 255, 255, 255, 255, 255, 244, 1, 0, 0, 0, 0, 0, 0, 160, 134, 1,
                0, 0, 0, 0, 0, 0, 211, 206, 254, 255, 255, 255, 255
            ]
        );
        let vp: Vec<(&str, i16)> = vec![("s1", 10), ("s2", -10000)];
        check_binary_format!(vp, vec![2, 2, 115, 49, 10, 0, 2, 115, 50, 240, 216]);
    }
}
