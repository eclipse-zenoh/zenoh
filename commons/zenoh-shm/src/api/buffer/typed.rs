//
// Copyright (c) 2025 ZettaScale Technology
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
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use zenoh_core::bail;
use zenoh_result::{IError, ZResult};

use crate::{
    api::{
        buffer::{
            traits::{ResideInShm, ShmBuf, ShmBufIntoImmut, ShmBufMut, ShmBufUnsafeMut},
            zshm::{zshm, ZShm},
            zshmmut::{zshmmut, ZShmMut},
        },
        provider::memory_layout::BuildLayout,
    },
    ShmBufInner,
};

/// Wrapper for SHM buffer types that is used for safe typed access to SHM data
pub struct Typed<T: ?Sized, Tbuf> {
    buf: Tbuf,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized, Tbuf: Clone> Clone for Typed<T, Tbuf> {
    fn clone(&self) -> Self {
        Self {
            buf: self.buf.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized, Tbuf> Typed<T, Tbuf> {
    /// Get the underlying SHM buffer
    pub fn inner(&self) -> &Tbuf {
        &self.buf
    }

    /// Convert into underlying SHM buffer
    pub fn into_inner(self) -> Tbuf {
        self.buf
    }

    /// #SAFETY: this is safe if buf's layout is compatible with T layout
    pub(crate) unsafe fn new_unchecked(buf: Tbuf) -> Self {
        Self {
            buf,
            _phantom: PhantomData,
        }
    }
}

impl<T: ResideInShm> From<Typed<T, ZShmMut>> for Typed<T, ZShm> {
    fn from(value: Typed<T, ZShmMut>) -> Self {
        Self {
            buf: value.buf.into(),
            _phantom: PhantomData,
        }
    }
}

impl<T: ResideInShm> TryFrom<Typed<T, ZShm>> for Typed<T, ZShmMut> {
    type Error = Typed<T, ZShm>;

    fn try_from(value: Typed<T, ZShm>) -> Result<Self, Self::Error> {
        Ok(Self {
            buf: value.buf.try_into().map_err(|e| Typed::<T, ZShm> {
                buf: e,
                _phantom: PhantomData,
            })?,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<Typed<T, &'a zshm>> for Typed<T, &'a zshmmut> {
    type Error = ();

    fn try_from(value: Typed<T, &'a zshm>) -> Result<Self, Self::Error> {
        Ok(Self {
            buf: value.buf.try_into()?,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<Typed<T, &'a mut zshm>> for Typed<T, &'a mut zshmmut> {
    type Error = ();

    fn try_from(value: Typed<T, &'a mut zshm>) -> Result<Self, Self::Error> {
        Ok(Self {
            buf: value.buf.try_into()?,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<Typed<T, &'a mut ZShm>> for Typed<T, &'a mut zshmmut> {
    type Error = ();

    fn try_from(value: Typed<T, &'a mut ZShm>) -> Result<Self, Self::Error> {
        Ok(Self {
            buf: value.buf.try_into()?,
            _phantom: PhantomData,
        })
    }
}

impl<T: ResideInShm> From<Typed<T, &zshmmut>> for Typed<T, &zshm> {
    fn from(value: Typed<T, &zshmmut>) -> Self {
        Self {
            buf: value.buf.into(),
            _phantom: PhantomData,
        }
    }
}

impl<T: ResideInShm> From<Typed<T, &mut zshmmut>> for Typed<T, &mut zshm> {
    fn from(value: Typed<T, &mut zshmmut>) -> Self {
        Self {
            buf: value.buf.into(),
            _phantom: PhantomData,
        }
    }
}

impl<T: ResideInShm> TryFrom<ZShm> for Typed<T, ZShm> {
    type Error = (ZShm, Box<dyn IError + Send + Sync + 'static>);

    fn try_from(value: ZShm) -> Result<Self, Self::Error> {
        match can_transmute::<T>(&value.inner) {
            Ok(_) => Ok(Self {
                buf: value,
                _phantom: PhantomData,
            }),
            Err(e) => Err((value, e)),
        }
    }
}

impl<T: ResideInShm> TryFrom<ZShmMut> for Typed<T, ZShmMut> {
    type Error = (ZShmMut, Box<dyn IError + Send + Sync + 'static>);

    fn try_from(value: ZShmMut) -> Result<Self, Self::Error> {
        match can_transmute::<T>(&value.inner) {
            Ok(_) => Ok(Self {
                buf: value,
                _phantom: PhantomData,
            }),
            Err(e) => Err((value, e)),
        }
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a zshm> for Typed<T, &'a zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a mut zshm> for Typed<T, &'a mut zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a mut zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a zshmmut> for Typed<T, &'a zshmmut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshmmut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a mut zshmmut> for Typed<T, &'a mut zshmmut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a mut zshmmut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<T: ResideInShm, Tbuf: ShmBuf<[u8]>> Deref for Typed<T, Tbuf> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: this is safe because we check transmute safety when constructing self
        unsafe { &*(self.buf.as_ref().as_ptr() as *const T) }
    }
}

impl<T: ResideInShm, Tbuf: ShmBuf<[u8]>> AsRef<T> for Typed<T, Tbuf> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ResideInShm, Tbuf: ShmBufMut<[u8]>> DerefMut for Typed<T, Tbuf> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: this is safe because we check transmute safety when constructing self
        unsafe { &mut *(self.buf.as_mut().as_mut_ptr() as *mut T) }
    }
}

impl<T: ResideInShm, Tbuf: ShmBufMut<[u8]>> AsMut<T> for Typed<T, Tbuf> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: ResideInShm, Tbuf: ShmBuf<[u8]>> ShmBuf<T> for Typed<T, Tbuf> {
    fn is_valid(&self) -> bool {
        self.buf.is_valid()
    }
}

impl<T: ResideInShm, Tbuf: ShmBufMut<[u8]>> ShmBufMut<T> for Typed<T, Tbuf> {}

impl<T: ResideInShm, Tbuf: ShmBufUnsafeMut<[u8]>> ShmBufUnsafeMut<T> for Typed<T, Tbuf> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut T {
        &mut *(self.buf.as_mut_unchecked().as_mut_ptr() as *mut T)
    }
}

impl<T: ResideInShm, Tbuf: ShmBufIntoImmut<[u8]>> ShmBufIntoImmut<T> for Typed<T, Tbuf> {
    type ImmutBuf = Typed<T, Tbuf::ImmutBuf>;

    fn into_immut(self) -> Self::ImmutBuf {
        Typed {
            buf: self.buf.into_immut(),
            _phantom: PhantomData,
        }
    }
}

fn can_transmute<T: ResideInShm>(value: &ShmBufInner) -> ZResult<()> {
    let slice = value.as_ref();

    let layout = BuildLayout::for_type::<T>();

    if slice.len() != layout.layout().size().get() {
        bail!(
            "Slice length does not match type size: expected {}, got {}",
            layout.layout().size().get(),
            slice.len()
        );
    }

    if (slice.as_ptr() as usize) % std::mem::align_of::<T>() != 0 {
        bail!(
            "Slice alignment does not match type alignment: expected {}",
            std::mem::align_of::<T>()
        );
    }

    Ok(())
}
