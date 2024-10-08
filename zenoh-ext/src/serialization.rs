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

pub trait Serialize {
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

pub trait Deserialize: Sized {
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

pub fn z_serialize<T: Serialize + ?Sized>(t: &T) -> ZBytes {
    let mut serializer = ZSerializer::new();
    serializer.serialize(t);
    serializer.finish()
}

pub fn z_deserialize<T: Deserialize>(zbytes: &ZBytes) -> Result<T, ZDeserializeError> {
    let mut deserializer = ZDeserializer::new(zbytes);
    let t = T::deserialize(&mut deserializer)?;
    if !deserializer.done() {
        return Err(ZDeserializeError);
    }
    Ok(t)
}

#[derive(Debug)]
pub struct ZSerializer(ZBytesWriter);

impl ZSerializer {
    pub fn new() -> Self {
        Self(ZBytes::writer())
    }

    pub fn serialize<T: Serialize>(&mut self, t: T) {
        t.serialize(self)
    }

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

    pub fn serialize_n<T: Serialize>(&mut self, ts: &[T]) {
        T::serialize_n(ts, self);
    }

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

#[derive(Debug)]
pub struct ZDeserializer<'a>(ZBytesReader<'a>);

impl<'a> ZDeserializer<'a> {
    pub fn new(zbytes: &'a ZBytes) -> Self {
        Self(zbytes.reader())
    }

    pub fn done(&self) -> bool {
        self.0.is_empty()
    }

    pub fn deserialize<T: Deserialize>(&mut self) -> Result<T, ZDeserializeError> {
        T::deserialize(self)
    }

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
    use zenoh::time::{Timestamp, TimestampId, NTP64};

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

    #[test]
    fn timestamp_serialization() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().into();
        let timestamp = Timestamp::new(now, TimestampId::rand());
        let (NTP64(ts), id) = (timestamp.get_time(), timestamp.get_id().to_le_bytes());
        let payload = z_serialize(&(ts, id));
        let (ts, id) = z_deserialize::<(_, [u8; 16])>(&payload).unwrap();
        let timestamp_out = Timestamp::new(NTP64(ts), TimestampId::try_from(&id).unwrap());
        assert_eq!(timestamp, timestamp_out);
    }
}
