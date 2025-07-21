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

use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use zenoh_result::{IError, ZResult};

use crate::{
    api::buffer::{
        traits::{ResideInShm, ShmBuf, ShmBufMut, ShmBufUnsafeMut},
        zshm::{zshm, ZShm},
        zshmmut::{zshmmut, ZShmMut},
    },
    ShmBufInner,
};

fn can_transmute<T: zerocopy::KnownLayout + zerocopy::FromBytes>(
    value: &ShmBufInner,
) -> ZResult<()> {
    let slice = value.as_ref();
    let _ = T::read_from_bytes(slice).map_err(|e| format!("Error transmutting: {e}"))?;
    Ok(())
}

pub struct Typed<T: ?Sized, Tbuf: ShmBuf<[u8]>> {
    buf: Tbuf,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized, Tbuf: ShmBuf<[u8]>> Typed<T, Tbuf> {
    pub fn unwrap(self) -> Tbuf {
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

impl<T: ResideInShm + zerocopy::FromBytes> TryFrom<ZShm> for Typed<T, ZShm> {
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

impl<T: ResideInShm + zerocopy::FromBytes> TryFrom<ZShmMut> for Typed<T, ZShmMut> {
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

impl<'a, T: ResideInShm + zerocopy::FromBytes> TryFrom<&'a zshm> for Typed<T, &'a zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm + zerocopy::FromBytes> TryFrom<&'a mut zshm> for Typed<T, &'a mut zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a mut zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm + zerocopy::FromBytes> TryFrom<&'a zshmmut> for Typed<T, &'a zshmmut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshmmut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.inner).map(|_| Self {
            buf: value,
            _phantom: PhantomData,
        })
    }
}

impl<'a, T: ResideInShm + zerocopy::FromBytes> TryFrom<&'a mut zshmmut>
    for Typed<T, &'a mut zshmmut>
{
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
