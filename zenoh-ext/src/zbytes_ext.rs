use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::Hash,
    io::{Read, Write},
    marker::PhantomData,
    ptr,
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

pub trait Serialize {
    fn write_into(&self, writer: &mut ZBytesWriter);
    #[doc(hidden)]
    fn write_slice_into(slice: &[Self], writer: &mut ZBytesWriter)
    where
        Self: Sized,
    {
        writer.write_iter(slice)
    }
}
impl<T: Serialize> Serialize for &T {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        T::write_into(*self, writer)
    }
}

pub trait Deserialize: Sized {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError>;
    #[doc(hidden)]
    fn read_slice_from(reader: &mut ZBytesReader) -> Result<Box<[Self]>, ZDeserializeError> {
        reader.read_iter::<Self>()?.collect()
    }
}

pub trait ZBytesExt {
    fn serialize<T: Serialize + ?Sized>(t: &T) -> Self;
    fn deserialize<T: Deserialize>(&self) -> Result<T, ZDeserializeError>;
}

impl ZBytesExt for ZBytes {
    fn serialize<T: Serialize + ?Sized>(t: &T) -> Self {
        let mut zbytes = ZBytes::new();
        t.write_into(&mut zbytes.writer());
        zbytes
    }

    fn deserialize<T: Deserialize>(&self) -> Result<T, ZDeserializeError> {
        let mut reader = self.reader();
        let t = T::read_from(&mut reader)?;
        if !reader.is_empty() {
            return Err(ZDeserializeError);
        }
        Ok(t)
    }
}

pub trait ZBytesWriterExt<'a> {
    fn write_iter<T: Serialize, I: IntoIterator<Item = T>>(&mut self, iter: I)
    where
        I::IntoIter: ExactSizeIterator;
}

impl<'a> ZBytesWriterExt<'a> for ZBytesWriter<'a> {
    fn write_iter<T: Serialize, I: IntoIterator<Item = T>>(&mut self, iter: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        let iter = iter.into_iter();
        self.write_vle(iter.len() as u64);
        for t in iter {
            t.write_into(self)
        }
    }
}

pub trait ZBytesReaderExt<'a> {
    fn read_iter<T: Deserialize>(&mut self) -> Result<ZReadIter<'a, '_, T>, ZDeserializeError>;
}
impl<'a> ZBytesReaderExt<'a> for ZBytesReader<'a> {
    fn read_iter<T: Deserialize>(&mut self) -> Result<ZReadIter<'a, '_, T>, ZDeserializeError> {
        let len = self.read_vle().ok_or(ZDeserializeError)? as usize;
        Ok(ZReadIter {
            reader: self,
            len,
            _phantom: PhantomData,
        })
    }
}

pub struct ZReadIter<'a, 'b, T: Deserialize> {
    reader: &'b mut ZBytesReader<'a>,
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
        Some(T::read_from(self.reader))
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

macro_rules! impl_num {
    ($($ty:ty),* $(,)?) => {$(
        impl Serialize for $ty {
            fn write_into(&self, writer: &mut ZBytesWriter) {
                writer.write(&(*self).to_le_bytes()).unwrap();
            }
            fn write_slice_into(slice: &[Self], writer: &mut ZBytesWriter) where Self: Sized {
                if cfg!(target_endian = "little") || std::mem::size_of::<Self>() == 1 {
                    writer.write_vle(slice.len() as u64);
                    // SAFETY: transmuting numeric types to their little endian bytes is safe
                    writer.write(unsafe { slice.align_to().1 }).unwrap();
                } else {
                    writer.write_iter(slice);
                }
            }
        }
        impl Deserialize for $ty {
            fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
                let mut buf = [0; { std::mem::size_of::<Self>() }];
                reader.read(&mut buf).or(Err(ZDeserializeError))?;
                Ok(<$ty>::from_le_bytes(buf))
            }
            fn read_slice_from(reader: &mut ZBytesReader) -> Result<Box<[Self]>, ZDeserializeError> {
                let size = std::mem::size_of::<Self>();
                if cfg!(target_endian = "little") || size == 1 {
                    let len = reader.read_vle().ok_or(ZDeserializeError)? as usize;
                    let total_size = len * size;
                    let mut buf = std::mem::ManuallyDrop::new(vec![0; total_size].into_boxed_slice());
                    reader.read(&mut buf).or(Err(ZDeserializeError))?;
                    // SAFETY: transmuting numeric types from their little endian bytes is safe
                    Ok(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr().cast(), len)) })
                } else {
                    reader.read_iter::<Self>()?.collect()
                }
            }
        }
    )*};
}
impl_num!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);

impl Serialize for bool {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        (*self as u8).write_into(writer);
    }
}
impl Deserialize for bool {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        match u8::read_from(reader)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ZDeserializeError),
        }
    }
}

impl<T: Serialize> Serialize for [T] {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        T::write_slice_into(self, writer)
    }
}
impl<'a, T: Serialize + 'a> Serialize for Cow<'a, [T]>
where
    [T]: ToOwned,
{
    fn write_into(&self, writer: &mut ZBytesWriter) {
        T::write_slice_into(self, writer)
    }
}
impl<T: Serialize> Serialize for Box<[T]> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        T::write_slice_into(self, writer)
    }
}
impl<T: Deserialize> Deserialize for Box<[T]> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        T::read_slice_from(reader)
    }
}
impl<T: Serialize> Serialize for Vec<T> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        T::write_slice_into(self, writer)
    }
}
impl<T: Deserialize> Deserialize for Vec<T> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        T::read_slice_from(reader).map(Into::into)
    }
}
impl<T: Serialize + Eq + Hash> Serialize for HashSet<T> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        writer.write_iter(self);
    }
}
impl<T: Deserialize + Eq + Hash> Deserialize for HashSet<T> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        reader.read_iter::<T>()?.collect()
    }
}
impl<T: Serialize + Ord> Serialize for BTreeSet<T> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        writer.write_iter(self);
    }
}
impl<T: Deserialize + Ord> Deserialize for BTreeSet<T> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        reader.read_iter::<T>()?.collect()
    }
}
impl<K: Serialize + Eq + Hash, V: Serialize> Serialize for HashMap<K, V> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        writer.write_iter(self);
    }
}
impl<K: Deserialize + Eq + Hash, V: Deserialize> Deserialize for HashMap<K, V> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        reader.read_iter::<(K, V)>()?.collect()
    }
}
impl<K: Serialize + Ord, V: Serialize> Serialize for BTreeMap<K, V> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        writer.write_iter(self);
    }
}
impl<K: Deserialize + Ord, V: Deserialize> Deserialize for BTreeMap<K, V> {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        reader.read_iter::<(K, V)>()?.collect()
    }
}
impl Serialize for str {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        self.as_bytes().write_into(writer);
    }
}
impl Serialize for Cow<'_, str> {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        self.as_bytes().write_into(writer);
    }
}
impl Serialize for String {
    fn write_into(&self, writer: &mut ZBytesWriter) {
        self.as_bytes().write_into(writer);
    }
}
impl Deserialize for String {
    fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
        String::from_utf8(Deserialize::read_from(reader)?).or(Err(ZDeserializeError))
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
            fn write_into(&self, writer: &mut ZBytesWriter) {
                $(self.$i.write_into(writer);)*
            }
        }
        #[allow(unused)]
        impl<$($ty: Deserialize),*> Deserialize for ($($ty,)*) {
            fn read_from(reader: &mut ZBytesReader) -> Result<Self, ZDeserializeError> {
                Ok(($($ty::read_from(reader)?,)*))
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
