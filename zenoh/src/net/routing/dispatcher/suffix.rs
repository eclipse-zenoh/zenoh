use std::{
    alloc,
    alloc::{handle_alloc_error, Layout},
    borrow::Borrow,
    collections::HashSet,
    hash::{Hash, Hasher},
    mem,
    mem::{ManuallyDrop, MaybeUninit},
    ops::Deref,
    ptr,
    ptr::{addr_of_mut, NonNull},
    slice,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
};

static SUFFIX_CACHE: OnceLock<Mutex<HashSet<ArcSuffix>>> = OnceLock::new();

// Use repr C to be able to use bytes as a DST field,
// and align(2) for pointer tagging
#[repr(C, align(2))]
struct ArcSuffixInner {
    rc: AtomicUsize,
    len: usize,
    bytes: [u8; 0],
}

impl ArcSuffixInner {
    fn layout(len: usize) -> Layout {
        let array = Layout::array::<u8>(len).unwrap();
        Layout::new::<ArcSuffixInner>().extend(array).unwrap().0
    }
}

struct ArcSuffix(NonNull<ArcSuffixInner>);

// SAFETY: the inner pointer is synchronized using the reference counter
unsafe impl Send for ArcSuffix {}
// SAFETY: the inner pointer is synchronized using the reference counter
unsafe impl Sync for ArcSuffix {}

impl ArcSuffix {
    fn new(s: &str) -> Self {
        let mut cache = SUFFIX_CACHE.get_or_init(Default::default).lock().unwrap();
        if let Some(suffix) = cache.get(s) {
            return suffix.clone();
        }
        let layout = ArcSuffixInner::layout(s.len());
        // SAFETY: layout is not zero-sized.
        let Some(inner_ptr): Option<NonNull<ArcSuffixInner>> =
            NonNull::new(unsafe { alloc::alloc(layout) }.cast())
        else {
            handle_alloc_error(layout);
        };
        let inner = ArcSuffixInner {
            rc: AtomicUsize::new(1),
            len: s.len(),
            bytes: [],
        };
        unsafe {
            ptr::write(inner_ptr.as_ptr(), inner);
            let bytes = addr_of_mut!((*inner_ptr.as_ptr()).bytes).cast();
            ptr::copy_nonoverlapping(s.as_ptr(), bytes, s.len());
        };
        let suffix = ArcSuffix(inner_ptr);
        let suffix_clone = suffix.clone();
        cache.insert(suffix);
        suffix_clone
    }

    fn as_str(&self) -> &str {
        let inner = self.0.as_ptr();
        // SAFETY: bytes have been initialized with a valid string
        unsafe {
            let bytes = addr_of_mut!((*inner).bytes).cast();
            std::str::from_utf8_unchecked(slice::from_raw_parts(bytes, (*inner).len))
        }
    }
}

impl Drop for ArcSuffix {
    fn drop(&mut self) {
        // SAFETY: the inner pointer is guaranteed to be valid as long as
        // the arc is valid, and there is no concurrent mutation as we are
        // using a shared reference.
        let inner = unsafe { self.0.as_mut() };
        // When there is only the cached instance left, remove it from the cache
        if inner.rc.fetch_sub(1, Ordering::Release) != 2 {
            return;
        }
        let mut cache = SUFFIX_CACHE.get().unwrap().lock().unwrap();
        // See `Arc::drop` documentation for the ordering
        if inner.rc.load(Ordering::Acquire) == 1 {
            assert!(cache.remove(self));
            // SAFETY: the arc was allocated with this layout
            unsafe { alloc::dealloc(self.0.as_ptr().cast(), ArcSuffixInner::layout(inner.len)) };
        }
    }
}

impl Clone for ArcSuffix {
    fn clone(&self) -> Self {
        // SAFETY: the inner pointer is guaranteed to be valid as long as
        // the arc is valid, and there is no concurrent mutation as we are
        // using a shared reference.
        let inner = unsafe { self.0.as_ref() };
        // See `Arc::clone` documentation
        let old_size = inner.rc.fetch_add(1, Ordering::Relaxed);
        const MAX_REFCOUNT: usize = isize::MAX as usize;
        if old_size > MAX_REFCOUNT {
            std::process::abort();
        }
        Self(self.0)
    }
}

impl Borrow<str> for ArcSuffix {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for ArcSuffix {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ArcSuffix {}

impl Hash for ArcSuffix {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

pub const INLINED_MAX_LENGTH: usize = mem::size_of::<usize>() - 1;
pub const INLINED_TAG: u8 = 0x01;
pub const INLINED_LENGTH_SHIFT: usize = 1;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct InlinedSuffix {
    #[cfg(target_endian = "little")]
    tagged_len: u8,
    bytes: [MaybeUninit<u8>; INLINED_MAX_LENGTH],
    #[cfg(target_endian = "big")]
    tagged_len: u8,
}

impl InlinedSuffix {
    fn new(s: &str) -> Option<Self> {
        if s.len() > INLINED_MAX_LENGTH {
            return None;
        }
        let mut this = Self {
            tagged_len: INLINED_TAG | (s.len() << INLINED_LENGTH_SHIFT) as u8,
            // SAFETY: taken from nightly `MaybeUninit::uninit_array`
            bytes: unsafe { MaybeUninit::uninit().assume_init() },
        };
        let bytes = this.bytes.as_mut_ptr().cast();
        // SAFETY: writing non-overlapping uninitialized bytes
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), bytes, s.len()) }
        Some(this)
    }

    fn is_inlined(&self) -> bool {
        self.tagged_len & INLINED_TAG != 0
    }

    fn as_str(&self) -> &str {
        // SAFETY: the bytes have been initialized in `new` from a valid str
        unsafe {
            std::str::from_utf8_unchecked(slice::from_raw_parts(
                self.bytes.as_ptr().cast(),
                (self.tagged_len >> INLINED_LENGTH_SHIFT) as usize,
            ))
        }
    }
}

pub(crate) union Suffix {
    arc: ManuallyDrop<ArcSuffix>,
    inlined: InlinedSuffix,
}

impl Suffix {
    pub(crate) fn new(s: &str) -> Self {
        match InlinedSuffix::new(s) {
            Some(inlined) => Self { inlined },
            None => Self {
                arc: ManuallyDrop::new(ArcSuffix::new(s)),
            },
        }
    }
}

impl Deref for Suffix {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        unsafe {
            if self.inlined.is_inlined() {
                self.inlined.as_str()
            } else {
                self.arc.as_str()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::net::routing::dispatcher::suffix::{ArcSuffix, InlinedSuffix, Suffix};
    const SMALL_STR: &str = "small";
    const BIG_STR: &str = "big string";

    #[test]
    fn test_inlined() {
        let inlined = InlinedSuffix::new(SMALL_STR).unwrap();
        assert_eq!(inlined.as_str(), SMALL_STR);

        assert!(InlinedSuffix::new(BIG_STR).is_none())
    }

    #[test]
    fn test_arc() {
        let arc = ArcSuffix::new(BIG_STR);
        assert_eq!(arc.as_str(), BIG_STR);

        let arc2 = ArcSuffix::new(BIG_STR);
        assert_eq!(arc.0, arc2.0)
    }

    #[test]
    fn test_suffix() {
        for suffix in [SMALL_STR, BIG_STR] {
            assert_eq!(*Suffix::new(suffix), *suffix);
        }
    }
}
