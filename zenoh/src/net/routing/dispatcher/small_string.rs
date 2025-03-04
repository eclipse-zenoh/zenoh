use std::{
    mem,
    mem::{ManuallyDrop, MaybeUninit},
    ops::Deref,
    ptr::NonNull,
    slice,
};

#[repr(C)]
pub struct SmallStringAllocated {
    #[cfg(target_endian = "big")]
    len: usize,
    ptr: NonNull<u8>,
    #[cfg(target_endian = "little")]
    len: usize,
}

unsafe impl Send for SmallStringAllocated {}
unsafe impl Sync for SmallStringAllocated {}

impl SmallStringAllocated {
    fn new(s: Box<str>) -> Self {
        let mut s = ManuallyDrop::new(s);
        Self {
            ptr: NonNull::new(s.as_mut_ptr()).unwrap(),
            len: s.len(),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for SmallStringAllocated {
    fn drop(&mut self) {
        let slice = std::ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), self.len);
        unsafe { drop(Box::from_raw(slice)) }
    }
}

pub const INLINED_LENGTH: usize = 2 * mem::size_of::<usize>() - 1;
pub const INLINED_TAG: u8 = 0x80;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SmallStringInlined {
    #[cfg(target_endian = "big")]
    tagged_len: u8,
    bytes: [MaybeUninit<u8>; INLINED_LENGTH],
    #[cfg(target_endian = "little")]
    tagged_len: u8,
}

impl SmallStringInlined {
    fn new(s: &str) -> Option<Self> {
        if s.len() > INLINED_LENGTH {
            return None;
        }
        let mut this = SmallStringInlined {
            bytes: unsafe { MaybeUninit::uninit().assume_init() },
            tagged_len: INLINED_TAG | s.len() as u8,
        };
        let bytes = this.bytes.as_mut_ptr().cast();
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), bytes, s.len()) }
        Some(this)
    }

    fn as_bytes(&self) -> &[u8] {
        let len = self.tagged_len & !INLINED_TAG;
        let bytes = &self.bytes[..len as usize];
        unsafe { &*(bytes as *const _ as *const [u8]) }
    }
}

#[repr(C)]
pub(crate) union SmallString {
    allocated: ManuallyDrop<SmallStringAllocated>,
    inlined: SmallStringInlined,
}

impl SmallString {
    fn is_inlined(&self) -> bool {
        unsafe { self.inlined.tagged_len & INLINED_TAG != 0 }
    }
}

impl Drop for SmallString {
    fn drop(&mut self) {
        if !self.is_inlined() {
            unsafe { ManuallyDrop::drop(&mut self.allocated) };
        }
    }
}

impl Deref for SmallString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        unsafe {
            std::str::from_utf8_unchecked(if self.is_inlined() {
                self.inlined.as_bytes()
            } else {
                self.allocated.as_bytes()
            })
        }
    }
}

impl From<&str> for SmallString {
    fn from(value: &str) -> Self {
        match SmallStringInlined::new(value) {
            Some(inlined) => Self { inlined },
            None => value.to_string().into(),
        }
    }
}

impl From<String> for SmallString {
    fn from(value: String) -> Self {
        match SmallStringInlined::new(&value) {
            Some(inlined) => Self { inlined },
            None => Self {
                allocated: ManuallyDrop::new(SmallStringAllocated::new(value.into())),
            },
        }
    }
}
