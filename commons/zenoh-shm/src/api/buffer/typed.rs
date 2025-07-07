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

use crate::{
    api::buffer::{traits::{ResideInShm, ShmBuf, ShmBufMut}, zshm::{zshm, ZShm}, zshmmut::{zshmmut, ZShmMut}},
    ShmBufInner,
};
use zenoh_core::bail;
use zenoh_result::{ZResult, IError};

fn can_transmute<T: ResideInShm>(value: &ShmBufInner) -> ZResult<()> {
    let slice = value.as_ref();

    let ptr = slice.as_ptr();

    // check alignment
    let type_align = std::mem::align_of::<T>();
    if ((ptr as usize) % type_align) != 0 {
        bail!("Incompatible alignent");
    }

    // check size
    let type_size = std::mem::size_of::<T>();
    let size = slice.len();
    if type_size != size {
        bail!("Incompatible size");
    }

    Ok(())
}

pub struct Typed<T: ResideInShm, Tbuf: ShmBuf> {
    buf: Tbuf,
    _phantom: PhantomData<T>,
}



impl<T: ResideInShm> TryFrom<ZShm> for Typed<T, ZShm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: ZShm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<T: ResideInShm> TryFrom<ZShmMut> for Typed<T, ZShmMut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: ZShmMut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a zshm> for Typed<T, &'a zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a mut zshm> for Typed<T, &'a mut zshm> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a mut zshm) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a zshmmut> for Typed<T, &'a zshmmut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a zshmmut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<'a, T: ResideInShm> TryFrom<&'a mut zshmmut> for Typed<T, &'a mut zshmmut> {
    type Error = Box<dyn IError + Send + Sync + 'static>;

    fn try_from(value: &'a mut zshmmut) -> Result<Self, Self::Error> {
        can_transmute::<T>(&value.0)?;
        Ok(Self {
            buf: value,
            _phantom: PhantomData::default(),
        })
    }
}

impl<T: ResideInShm, Tbuf: ShmBuf> Deref for Typed<T, Tbuf> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.buf.as_ref().as_ptr() as *const T) }
    }
}

impl<T: ResideInShm, Tbuf: ShmBuf> AsRef<T> for Typed<T, Tbuf> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ResideInShm, Tbuf: ShmBufMut> DerefMut for Typed<T, Tbuf> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.buf.as_mut().as_mut_ptr() as *mut T) }
    }
}

impl<T: ResideInShm, Tbuf: ShmBufMut> AsMut<T> for Typed<T, Tbuf> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}
