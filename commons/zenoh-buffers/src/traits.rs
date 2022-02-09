use std::borrow::Cow;

pub mod buffer {
    use std::num::NonZeroUsize;
    pub trait ConstructibleBuffer {
        /// Constructs a split buffer that may accept `slice_capacity` segments without allocating.
        /// It may also accept receiving cached writes for `cache_capacity` bytes before needing to reallocate its cache.
        fn with_capacities(slice_capacity: usize, cache_capacity: usize) -> Self;
    }
    pub trait BoundedBuffer {
        /// Indicates how many bytes can still be written to the buffer before it may reject further writes.
        fn remaining_capacity(&self) -> usize;
    }

    pub trait CopyBuffer {
        /// Copies as much of `bytes` as possible inside its cache, returning the amount actually written.
        /// Will return `None` if the write was refused.
        fn write(&mut self, bytes: &[u8]) -> Option<NonZeroUsize>;
        fn write_byte(&mut self, byte: u8) -> Option<NonZeroUsize> {
            self.write(&[byte])
        }
    }
    pub trait Indexable {
        type Index;
        /// Returns an index-like mark that may be used for future operations at the current buffer-end
        fn get_index(&self) -> Self::Index;
        /// Replaces the data following `from` with the contents of `with`.
        ///
        /// This is an exact size operation, to avoid invalidating other marks that may have been made.
        fn replace(&mut self, from: &Self::Index, with: &[u8]) -> bool;
    }
    pub trait InsertBuffer<T> {
        /// Appends a slice to the buffer without copying its data.
        fn append(&mut self, slice: T);
    }
}

pub mod writer {
    use crate::buffer::InsertBuffer;

    pub trait Writer: AsRef<Self::Buffer> + AsMut<Self::Buffer> {
        type Buffer;
        fn copyless_write<T>(&mut self, slice: T)
        where
            Self::Buffer: InsertBuffer<T>,
        {
            self.as_mut().append(slice)
        }
    }
    pub trait BacktrackableWriter {
        fn mark(&mut self);
        fn revert(&mut self) -> bool;
    }
    pub trait HasWriter {
        type Writer: Writer;
        /// Returns the most appropriate writer for `self`
        fn writer(self) -> Self::Writer;
    }
}

pub trait SplitBuffer<'a> {
    type Slices: Iterator<Item = &'a [u8]> + ExactSizeIterator;
    fn slices(&'a self) -> Self::Slices;

    fn is_empty(&'a self) -> bool {
        self.slices().all(|s| s.is_empty())
    }
    fn len(&'a self) -> usize {
        self.slices().fold(0, |acc, it| acc + it.len())
    }
    fn contiguous(&'a self) -> Cow<'a, [u8]> {
        let mut slices = self.slices();
        match slices.len() {
            0 => Cow::Borrowed(b""),
            1 => Cow::Borrowed(slices.next().unwrap()),
            _ => Cow::Owned(slices.fold(Vec::new(), |mut acc, it| {
                acc.extend(it);
                acc
            })),
        }
    }
}

pub mod reader {
    pub trait Reader {
        fn read(&mut self, into: &mut [u8]) -> usize;
        fn read_exact(&mut self, into: &mut [u8]) -> bool;
        fn read_byte(&mut self) -> Option<u8> {
            let mut byte = 0;
            (self.read(std::slice::from_mut(&mut byte)) != 0).then(|| byte)
        }
    }
}
