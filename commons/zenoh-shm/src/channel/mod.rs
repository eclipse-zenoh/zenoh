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

use libc::memcpy;
use stabby::IStable;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::mem::{align_of, size_of};
use std::sync::atomic::{AtomicPtr, AtomicUsize};
use zenoh_core::zerror;
use zenoh_result::{ZError, ZResult};

use std::sync::atomic::Ordering::Relaxed;

#[repr(C, packed(1))]
struct MessageHeader {
    len: usize,
}

/*
________________________________________________________________________
| header |-opt-padding-|       elem        |       elem        | ..... |

*/

struct InnerLayout<T: IStable<ContainsIndirections = stabby::abi::B0>> {
    _phantom: PhantomData<T>,
}

impl<T: IStable<ContainsIndirections = stabby::abi::B0>> InnerLayout<T> {
    const fn header_with_padding() -> usize {
        size_of::<MessageHeader>() + Self::header_padding()
    }

    const fn header_padding() -> usize {
        if size_of::<MessageHeader>() > align_of::<T>() {
            return (align_of::<T>() - (size_of::<MessageHeader>() % align_of::<T>()))
                % align_of::<T>();
        } else if size_of::<MessageHeader>() < align_of::<T>() {
            return align_of::<T>() - size_of::<MessageHeader>();
        }
        0
    }

    #[inline(always)]
    fn byte_len(msg: &[T]) -> usize {
        std::mem::size_of_val(msg)
    }

    #[inline(always)]
    fn elem_len(byte_len: usize) -> usize {
        byte_len / size_of::<T>()
    }
}

pub struct ChannelData<'a, T: IStable<ContainsIndirections = stabby::abi::B0> + 'a> {
    // free data in storage units
    pub free: &'a mut AtomicUsize,

    pub pop_pos: usize,
    pub push_pos: usize,

    /*
    Shared data structure:
    ___________________________________________________
    | 1. self.data | 2. rollover | 3. --- | self.free |

    1:
        - aligned for T
        - size is a multiply of alignment
    2: same layout as 1
    3: padding to align self.free
    4: properly aligned

    */
    pub data: &'a mut [u8],

    _phantom: PhantomData<T>,
}

impl<'a, T: IStable<ContainsIndirections = stabby::abi::B0> + 'a> ChannelData<'a, T> {
    fn push(&mut self, msg: &[T]) -> bool {
        let bytes_to_store =
            InnerLayout::<T>::header_with_padding() + InnerLayout::<T>::byte_len(msg);

        if self.free.load(Relaxed) < bytes_to_store {
            return false;
        }

        let header = self.data[self.push_pos] as *mut u8 as *mut MessageHeader;

        unsafe {
            let data = ((&mut self.data[self.push_pos]) as *mut u8)
                .add(InnerLayout::<T>::header_with_padding());

            (*header).len = bytes_to_store;
            memcpy(
                data as *mut libc::c_void,
                msg.as_ptr() as *const libc::c_void,
                bytes_to_store - InnerLayout::<T>::header_with_padding(),
            );
        };
        self.free
            .fetch_sub(bytes_to_store, std::sync::atomic::Ordering::SeqCst);

        let new_pos = self.push_pos + bytes_to_store;
        if new_pos >= self.data.len() {
            self.push_pos = 0;
        } else {
            self.push_pos = new_pos;
        }

        true
    }

    fn pop(&mut self) -> Option<Vec<T>> {
        if self.data.len() == self.free.load(Relaxed) {
            return None;
        }

        let header = self.data[self.pop_pos] as *const MessageHeader;
        let len = unsafe { (*header).len };

        let mut result = Vec::with_capacity(InnerLayout::<T>::elem_len(
            len - InnerLayout::<T>::header_with_padding(),
        ));
        unsafe {
            let data = ((&self.data[self.pop_pos]) as *const u8)
                .add(InnerLayout::<T>::header_with_padding());

            memcpy(
                result.spare_capacity_mut().as_ptr() as *mut libc::c_void,
                data as *const libc::c_void,
                len - InnerLayout::<T>::header_with_padding(),
            );
            result.set_len(len - InnerLayout::<T>::header_with_padding());
        };

        self.free
            .fetch_add(len, std::sync::atomic::Ordering::SeqCst);

        let new_pos = self.pop_pos + len;
        if new_pos >= self.data.len() {
            self.pop_pos = 0;
        } else {
            self.pop_pos = new_pos;
        }

        Some(result)
    }
}

pub struct Channel<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> {
    data: ChannelData<'a, T>,
}

impl<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> Channel<'a, T> {
    pub fn create<TryIntoData>(data: TryIntoData) -> ZResult<Tx<'a, T>>
    where
        TryIntoData: TryInto<ChannelData<'a, T>>,
        <TryIntoData as TryInto<ChannelData<'a, T>>>::Error: Into<zenoh_result::Error>,
    {
        let data = data.try_into().map_err(|e| e.into())?;
        Ok(Tx { data })
    }

    pub fn open<TryIntoData>(data: TryIntoData) -> ZResult<Rx<'a, T>>
    where
        TryIntoData: TryInto<ChannelData<'a, T>>,
        <TryIntoData as TryInto<ChannelData<'a, T>>>::Error: Into<zenoh_result::Error>,
    {
        let data = data.try_into().map_err(|e| e.into())?;
        Ok(Rx { data })
    }
}

pub struct Tx<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> {
    data: ChannelData<'a, T>,
}

impl<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> Tx<'a, T> {
    pub fn send(&mut self, msg: &[T]) -> bool {
        self.data.push(msg)
    }
}

pub struct Rx<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> {
    data: ChannelData<'a, T>,
}

impl<'a, T: IStable<ContainsIndirections = stabby::abi::B0>> Rx<'a, T> {
    pub fn receive(&mut self) -> Option<Vec<T>> {
        self.data.pop()
    }
}
