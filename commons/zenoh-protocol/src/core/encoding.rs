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
use alloc::alloc;
use core::{
    fmt,
    mem::MaybeUninit,
    slice, str,
    sync::{
        atomic,
        atomic::{AtomicUsize, Ordering, Ordering::Release},
    },
};

pub type EncodingId = u16;

/// [`Encoding`] is a metadata that indicates how the data payload should be interpreted.
/// For wire-efficiency and extensibility purposes, Zenoh defines an [`Encoding`] as
/// composed of an unsigned integer prefix and a bytes schema. The actual meaning of the
/// prefix and schema are out-of-scope of the protocol definition. Therefore, Zenoh does not
/// impose any encoding mapping and users are free to use any mapping they like.
/// Nevertheless, it is worth highlighting that Zenoh still provides a default mapping as part
/// of the API as per user convenience. That mapping has no impact on the Zenoh protocol definition.
pub struct Encoding(EncodingIdOrInner);

/// This union embed either a raw encoding id, or an Arc-like [`EncodingInner`] pointer
/// Because [`EncodingInner`] has the same alignment as `usize`, the first bit of a valid
/// `*const EncodingInner` pointer must always be zero. We can then use this bit as the union
/// discriminant, by shifting the raw encoding id.
/// A special case is made for id 0, which doesn't need to be shifted because it also corresponds
/// to an invalid pointer (the null pointer).
union EncodingIdOrInner {
    shifted_id: usize,
    ptr: *const EncodingInner,
}

/// Flag to discriminate union variant, see [`EncodingIdOrInner`].
/// If the bit is set, then `EncodingIdOrInner` contains a raw encoding id.
const RAW_ENCODING_ID: usize = 1;

#[repr(C)]
struct EncodingInner {
    rc: AtomicUsize,
    schema_length: usize,
    id: EncodingId,
    schema: [u8; 0],
}

/// # Encoding field
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   id: z16   |S~
/// +---------------+
/// ~schema: <u8;z8>~  -- if S==1
/// +---------------+
/// ```
pub mod flag {
    pub const S: u32 = 1; // 0x01 Suffix    if S==1 then schema is present
}

impl Encoding {
    #[inline]
    pub const fn new(id: EncodingId) -> Self {
        let mut id = id as usize;
        if id > 0 {
            id = (id << RAW_ENCODING_ID) | 1;
        }
        Self(EncodingIdOrInner { shifted_id: id })
    }

    /// Returns a new [`Encoding`] object with default empty prefix ID.
    #[inline]
    pub const fn empty() -> Self {
        Self::new(0)
    }

    #[inline]
    pub fn id_and_schema(&self) -> (EncodingId, Option<&str>) {
        // SAFETY: accessing flagged_id is always safe as it is an usize
        let flagged_id = unsafe { self.0.shifted_id };
        if flagged_id == 0 {
            return (0, None);
        }
        if flagged_id & RAW_ENCODING_ID != 0 {
            return ((flagged_id >> 1) as EncodingId, None);
        }
        // SAFETY: the union doesn't have the id-only flag, so it must be a valid pointer
        let inner = unsafe { &*self.0.ptr };
        // SAFETY: `EncodingInner` was allocated as a DST with a str in place of schema.
        let schema = unsafe {
            str::from_utf8_unchecked(slice::from_raw_parts(
                inner.schema.as_ptr(),
                inner.schema_length,
            ))
        };
        (inner.id, Some(schema))
    }

    fn layout(schema_length: usize) -> alloc::Layout {
        let layout = alloc::Layout::new::<EncodingInner>();
        layout
            .extend(alloc::Layout::array::<u8>(schema_length).expect("Overflow"))
            .expect("Overflow")
            .0
    }

    pub fn with_schema(&self, schema: impl AsRef<str>) -> Self {
        let schema = schema.as_ref();
        let (id, old_schema) = self.id_and_schema();
        if old_schema.is_some_and(|s| s == schema) {
            return self.clone();
        }
        // SAFETY: layout has non-zero-size
        let inner = unsafe {
            &mut *(alloc::alloc(Self::layout(schema.len())) as *mut MaybeUninit<EncodingInner>)
        };
        inner.write(EncodingInner {
            rc: AtomicUsize::new(1),
            schema_length: schema.len(),
            id,
            schema: [],
        });
        // SAFETY: inner has been initialized
        let inner = unsafe { inner.assume_init_mut() };
        // SAFETY: `EncodingInner` was allocated as a DST with its layout extended
        // by the layout of the schema.
        unsafe { slice::from_raw_parts_mut(inner.schema.as_mut_ptr(), inner.schema_length) }
            .copy_from_slice(schema.as_bytes());
        Self(EncodingIdOrInner {
            ptr: (inner as *mut EncodingInner).cast_const(),
        })
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::empty()
    }
}

impl Clone for Encoding {
    fn clone(&self) -> Self {
        // SAFETY: accessing flagged_id is always safe as it is an usize
        let flagged_id = unsafe { self.0.shifted_id };
        if flagged_id == 0 || flagged_id & RAW_ENCODING_ID != 0 {
            return Self(EncodingIdOrInner {
                shifted_id: flagged_id,
            });
        }
        // SAFETY: the union doesn't have the id-only flag, so it must be a valid pointer
        let inner = unsafe { &*self.0.ptr };
        // See `Arc` code
        if inner.rc.fetch_add(1, Ordering::Relaxed) > (isize::MAX as usize) {
            #[cfg(feature = "std")]
            std::process::abort();
            // intrinsics::abort is not available on stable rust...
            // However, this situation should never be encountered in normal program, so panic,
            // hoping than panic handler will abort.
            #[cfg(not(feature = "std"))]
            panic!("This program is incredibly degenerate");
        }
        Self(EncodingIdOrInner { ptr: inner })
    }
}

impl fmt::Debug for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (id, schema) = self.id_and_schema();
        f.debug_struct("Encoding")
            .field("id", &id)
            .field("schema", &schema)
            .finish()
    }
}

impl PartialEq for Encoding {
    fn eq(&self, other: &Self) -> bool {
        self.id_and_schema() == other.id_and_schema()
    }
}

impl Eq for Encoding {}

impl Drop for Encoding {
    fn drop(&mut self) {
        // SAFETY: accessing flagged_id is always safe as it is an usize
        let flagged_id = unsafe { self.0.shifted_id };
        if flagged_id == 0 || flagged_id & RAW_ENCODING_ID != 0 {
            return;
        }
        // SAFETY: the union doesn't have the id-only flag, so it must be a valid pointer
        let inner = unsafe { &*self.0.ptr };
        // See `Arc` code
        if inner.rc.fetch_sub(1, Release) != 1 {
            return;
        }
        // See `Arc` code
        atomic::fence(Ordering::Acquire);
        let layout = Self::layout(inner.schema_length);
        // SAFETY: inner has been allocated with `alloc::alloc` and the same layout
        unsafe { alloc::dealloc((inner as *const EncodingInner).cast_mut().cast(), layout) }
    }
}

// SAFETY: Encoding implementation is synchronized with atomic reference counting
unsafe impl Send for Encoding {}
// SAFETY: Encoding implementation is synchronized with atomic reference counting
unsafe impl Sync for Encoding {}

impl Encoding {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let mut encoding = Encoding::new(rng.gen());
        if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            let schema: Vec<u8> = (0..len).map(|_| rng.gen()).collect();
            encoding = encoding.with_schema(String::from_utf8_lossy(&schema))
        }
        encoding
    }
}
