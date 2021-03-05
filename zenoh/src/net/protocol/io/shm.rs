//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf, ShmemError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

const MIN_CHUNK_SIZE: usize = 1024;
const ZENOH_SHM_PREFIX: &str = "zenoh_shm_pid";

/// The ChunkHeader is used to contain information about the chunk of shared memory.
/// The same header is used for both free and used chunk, two different lists maintain
/// the relevant chunks.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct ChunkHeader {
    pub(crate) ref_count: AtomicUsize,
    pub(crate) len: usize,
    pub(crate) prev: *mut ChunkHeader,
    pub(crate) next: *mut ChunkHeader,
}

impl ChunkHeader {
    fn init(ptr: *mut ChunkHeader, len: usize, next: *mut ChunkHeader, prev: *mut ChunkHeader) {
        unsafe {
            (*ptr).ref_count.store(0, Ordering::SeqCst);
            (*ptr).len = len;
            (*ptr).next = next;
            (*ptr).prev = prev;
        }
    }

    fn ref_count(ptr: *const ChunkHeader) -> usize {
        unsafe { (*ptr).ref_count.load(Ordering::SeqCst) }
    }

    fn inc_ref_count(ptr: *mut ChunkHeader) {
        unsafe {
            (*ptr).ref_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn dec_ref_count(ptr: *mut ChunkHeader) {
        unsafe {
            (*ptr).ref_count.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
#[repr(C)]
struct ChunkList {
    hd: AtomicPtr<ChunkHeader>,
    tl: AtomicPtr<ChunkHeader>,
    elems: usize,
}

// @TODO: There are some operations that we are not using currently and my be
// useful going forward.
#[allow(dead_code)]
impl ChunkList {
    fn empty() -> ChunkList {
        ChunkList {
            hd: AtomicPtr::new(std::ptr::null_mut()),
            tl: AtomicPtr::new(std::ptr::null_mut()),
            elems: 0,
        }
    }

    fn set_head(&mut self, ptr: *mut ChunkHeader) {
        self.hd.store(ptr, Ordering::SeqCst);
    }

    fn set_tail(&mut self, ptr: *mut ChunkHeader) {
        self.tl.store(ptr, Ordering::SeqCst);
    }

    fn head(&self) -> *mut ChunkHeader {
        self.hd.load(Ordering::SeqCst)
    }

    fn tail(&self) -> *mut ChunkHeader {
        self.tl.load(Ordering::SeqCst)
    }

    fn is_empty(&self) -> bool {
        self.head().is_null()
    }

    fn prepend(&mut self, e: *mut ChunkHeader) {
        unsafe {
            (*e).next = std::ptr::null_mut();
            (*e).prev = std::ptr::null_mut();
        }
        if self.is_empty() {
            self.set_head(e);
            self.set_tail(e);
        } else {
            unsafe {
                (*e).next = self.head();
                self.set_head(e);
                (*e).prev = std::ptr::null_mut();
            }
        }
    }

    fn append(&mut self, e: *mut ChunkHeader) {
        unsafe {
            (*e).next = std::ptr::null_mut();
            (*e).prev = std::ptr::null_mut();
            if self.is_empty() {
                self.set_head(e);
                self.set_tail(e);
            } else {
                (*e).prev = self.tail();
                (*e).next = std::ptr::null_mut();
                (*self.tail()).next = e;
                self.set_tail(e);
            }
            self.elems += 1;
        }
    }

    fn remove(&mut self, e: *mut ChunkHeader) {
        let oh = self.head();
        unsafe {
            if e == self.head() {
                self.set_head((*e).next);
                if oh == self.tail() {
                    self.set_tail(self.head());
                }
            } else if e == self.tail() {
                self.set_tail((*e).prev);
            } else {
                let next = (*e).next;
                (*(*e).prev).next = next;
            }
        }
    }

    fn elems(&self) -> usize {
        self.elems
    }

    fn next(e: *mut ChunkHeader) -> *mut ChunkHeader {
        unsafe { (*e).next }
    }

    fn insert_after(e: *mut ChunkHeader, ne: *mut ChunkHeader) {
        unsafe {
            let next = (*e).next;
            (*ne).prev = e;
            (*e).next = e;
            (*ne).next = next;
        }
    }

    fn insert_before(e: *mut ChunkHeader, ne: *mut ChunkHeader) {
        unsafe {
            let prev = (*e).prev;
            (*prev).next = ne;
            (*ne).prev = prev;
            (*ne).next = e;
            (*e).prev = ne;
        }
    }

    fn find_best_fit(&self, _len: usize) -> Option<*mut ChunkHeader> {
        None
    }

    fn is_head(lst: *mut ChunkList, e: *mut ChunkHeader) -> bool {
        unsafe { (*lst).head() == e }
    }

    fn is_tail(lst: *mut ChunkList, e: *mut ChunkHeader) -> bool {
        unsafe { (*lst).tail() == e }
    }

    fn find_first_fit(&self, len: usize) -> Option<*mut ChunkHeader> {
        let mut chunk = self.head();
        loop {
            if unsafe { (*chunk).len >= len } {
                return Some(chunk);
            }
            chunk = unsafe { (*chunk).next };
            if chunk.is_null() {
                return None;
            }
        }
    }
}

impl std::fmt::Debug for ChunkList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f.write_str("ChunkList [");
        let mut current = self.head();
        loop {
            if current.is_null() {
                return f.write_str("]");
            }
            let _ = f.write_fmt(format_args!("{:?}", unsafe { &*current }));
            current = unsafe { (*current).next };
        }
    }
}

#[repr(C)]
#[derive(Debug)]
struct SharedMemoryHeader {
    free_list: ChunkList,
    used_list: ChunkList,
    capacity: usize,
    used: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SharedMemoryBufInfo {
    pub header_offset: usize,
    pub data_offset: usize,
    pub length: usize,
    pub shm_manager: String,
    pub kind: u8,
}

impl SharedMemoryBufInfo {
    pub fn new(
        header_offset: usize,
        data_offset: usize,
        length: usize,
        manager: String,
        kind: u8,
    ) -> SharedMemoryBufInfo {
        SharedMemoryBufInfo {
            header_offset,
            data_offset,
            length,
            shm_manager: manager,
            kind,
        }
    }
}

impl Clone for SharedMemoryBufInfo {
    fn clone(&self) -> SharedMemoryBufInfo {
        SharedMemoryBufInfo {
            shm_manager: self.shm_manager.clone(),
            kind: self.kind,
            header_offset: self.header_offset,
            data_offset: self.data_offset,
            length: self.length,
        }
    }
}
pub struct AtomicSharedMemoryBuf {
    pub(crate) chunk_header: AtomicPtr<ChunkHeader>,
    pub(crate) buf: AtomicPtr<u8>,
    pub(crate) len: usize,
    pub(crate) info: SharedMemoryBufInfo,
}

impl Clone for AtomicSharedMemoryBuf {
    fn clone(&self) -> Self {
        let hp = self.chunk_header.load(Ordering::SeqCst);
        let bp = self.buf.load(Ordering::SeqCst);
        ChunkHeader::inc_ref_count(hp);
        AtomicSharedMemoryBuf {
            chunk_header: AtomicPtr::new(hp),
            buf: AtomicPtr::new(bp),
            len: self.len,
            info: self.info.clone(),
        }
    }
}

pub struct SharedMemoryBuf {
    pub(crate) chunk_header: AtomicPtr<ChunkHeader>,
    pub(crate) buf: AtomicPtr<u8>,
    pub(crate) len: usize,
    pub(crate) info: SharedMemoryBufInfo,
}

impl SharedMemoryBuf {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_kind(&self) -> u8 {
        self.info.kind
    }

    pub fn set_kind(&mut self, v: u8) {
        self.info.kind = v
    }

    pub fn owner(&self) -> String {
        self.info.shm_manager.clone()
    }

    pub fn ref_count(&self) -> usize {
        let hp = self.chunk_header.load(Ordering::SeqCst);
        ChunkHeader::ref_count(hp)
    }

    pub fn inc_ref_count(&self) {
        let hp = self.chunk_header.load(Ordering::SeqCst);
        ChunkHeader::inc_ref_count(hp);
    }

    pub fn dec_ref_count(&self) {
        let hp = self.chunk_header.load(Ordering::SeqCst);
        ChunkHeader::dec_ref_count(hp);
    }

    pub fn as_slice(&self) -> &[u8] {
        let bp = self.buf.load(Ordering::SeqCst);
        unsafe { std::slice::from_raw_parts(bp, self.len) }
    }

    ///
    /// Get a mutable slice.
    ///
    /// # Safety
    /// This operation is marked unsafe since we cannot guarantee the single mutable reference
    /// across multiple processes. Thus if you use it, and you'll inevitable have to use it,
    /// you have to keep in mind that if you have multiple process retrieving a mutable slice
    /// you may get into concurrent writes. That said, if you have a serial pipeline and
    /// the buffer is flowing through the pipeline this will not create any issues.
    ///
    /// In short, whilst this operation is marked as unsafe, you are safe if you can
    /// guarantee that your in applications only one process at the time will actually write.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        let bp = self.buf.load(Ordering::SeqCst);
        std::slice::from_raw_parts_mut(bp, self.len)
    }
}

impl Drop for SharedMemoryBuf {
    fn drop(&mut self) {
        self.dec_ref_count();
    }
}

impl Clone for SharedMemoryBuf {
    fn clone(&self) -> Self {
        self.inc_ref_count();
        let hp = self.chunk_header.load(Ordering::SeqCst);
        let bp = self.buf.load(Ordering::SeqCst);
        SharedMemoryBuf {
            chunk_header: AtomicPtr::new(hp),
            buf: AtomicPtr::new(bp),
            len: self.len,
            info: self.info.clone(),
        }
    }
}

pub struct SharedMemoryManager {
    segment_path: String,
    size: usize,
    offset: usize,
    own_segment: Shmem,
    segments: HashMap<String, Shmem>,
    header: SharedMemoryHeader,
}

unsafe impl Send for SharedMemoryManager {}

impl SharedMemoryManager {
    /// Creates a new SharedMemoryManager managing allocations of a region of the
    /// given size.
    pub fn new(id: String, size: usize) -> Result<SharedMemoryManager, ShmemError> {
        let mut temp_dir = std::env::temp_dir();
        let file_name: String = format!("{}_{}", ZENOH_SHM_PREFIX, id);
        temp_dir.push(file_name);
        let path: String = temp_dir.to_str().unwrap().to_string();
        log::trace!("Creating file at: {}", path);
        let shmem = match ShmemConf::new().size(size).flink(path.clone()).create() {
            Ok(m) => m,
            Err(ShmemError::LinkExists) => {
                log::trace!("Shared Memory already exists, opening it");
                ShmemConf::new().flink(path.clone()).open()?
            }
            Err(e) => return Err(e),
        };
        let chunk_header_size = std::mem::size_of::<ChunkHeader>();
        let base_ptr = shmem.as_ptr();
        let mut header = SharedMemoryHeader {
            free_list: ChunkList::empty(),
            used_list: ChunkList::empty(),
            capacity: size - chunk_header_size,
            used: 0,
        };

        let chunk = base_ptr as *mut ChunkHeader;
        ChunkHeader::init(
            chunk,
            header.capacity,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        header.free_list.append(chunk);

        log::trace!("Creating SharedMemoryManager for {:?}", base_ptr);
        Ok(SharedMemoryManager {
            segment_path: path,
            size,
            offset: 0,
            own_segment: shmem,
            segments: HashMap::new(),
            header,
        })
    }

    pub fn alloc(&mut self, len: usize) -> Option<SharedMemoryBuf> {
        let available = self.size - self.offset;
        if available > len {
            // We may have a slot, but not sure because this only checks the total
            // memory available not the biggest chunk.
            // @TODO: Potentially we could keep track of the biggest chunck to avoid,
            // in some cases, walking the free list.
            unsafe {
                match self.header.free_list.find_first_fit(len) {
                    Some(chunk) => {
                        self.header.free_list.remove(chunk);
                        let ptr = chunk as *mut u8;
                        let header_offset = ptr.offset_from(self.own_segment.as_ptr()) as usize;
                        let data_offset = header_offset + std::mem::size_of::<ChunkHeader>();
                        let data_ptr = ptr.add(data_offset);
                        let manager = self.segment_path.clone();
                        let info =
                            SharedMemoryBufInfo::new(header_offset, data_offset, len, manager, 0);
                        ChunkHeader::inc_ref_count(chunk);
                        // Now we need to decide if it is worth to split this chunk
                        let remaining = (*chunk).len - len;
                        if remaining >= MIN_CHUNK_SIZE {
                            // The left-over is sufficient to justify creating another chunk
                            let free_chunk = data_ptr.add(len) as *mut ChunkHeader;
                            let fc_len = remaining - std::mem::size_of::<ChunkHeader>();
                            ChunkHeader::init(
                                free_chunk,
                                fc_len,
                                std::ptr::null_mut(),
                                std::ptr::null_mut(),
                            );
                            self.header.free_list.prepend(free_chunk);
                            // Update the chunk length
                            (*chunk).len = len;
                        }
                        self.header.used += (*chunk).len;
                        // Add chunk to the used list
                        self.header.used_list.append(chunk);
                        Some(SharedMemoryBuf {
                            chunk_header: AtomicPtr::new(chunk),
                            buf: AtomicPtr::new(data_ptr),
                            len,
                            info,
                        })
                    }
                    None => None,
                }
            }
        } else {
            None
        }
    }

    pub fn from_info(&mut self, info: SharedMemoryBufInfo) -> Option<SharedMemoryBuf> {
        // From info does not increment the reference count as it is assumed
        // that the sender of this buffer has incremented for us.
        match self.segments.get(&info.shm_manager) {
            Some(shm) => {
                let base_ptr = shm.as_ptr();
                let chunk_header = unsafe { base_ptr.add(info.header_offset) as *mut ChunkHeader };
                let buf = unsafe { base_ptr.add(info.data_offset) as *mut u8 };
                ChunkHeader::inc_ref_count(chunk_header);
                Some(SharedMemoryBuf {
                    chunk_header: AtomicPtr::new(chunk_header),
                    buf: AtomicPtr::new(buf),
                    len: info.length,
                    info,
                })
            }
            None => match ShmemConf::new().flink(&info.shm_manager).open() {
                Ok(shm) => {
                    log::trace!("Binding shared buffer to: {}", info.shm_manager);
                    let base_ptr = shm.as_ptr();
                    let chunk_header =
                        unsafe { base_ptr.add(info.header_offset) as *mut ChunkHeader };
                    let buf = unsafe { base_ptr.add(info.data_offset) as *mut u8 };
                    ChunkHeader::inc_ref_count(chunk_header);
                    self.segments.insert(info.shm_manager.clone(), shm);
                    Some(SharedMemoryBuf {
                        chunk_header: AtomicPtr::new(chunk_header),
                        buf: AtomicPtr::new(buf),
                        len: info.length,
                        info,
                    })
                }
                Err(e) => {
                    log::trace!(
                        "Unable to bind shared segment: {} -- {:?}",
                        info.shm_manager,
                        e
                    );
                    None
                }
            },
        }
    }

    /// Returns the amount of memory freed
    pub fn garbage_collect(&mut self) -> usize {
        log::debug!("Running Garbage Collector");
        let mut current = self.header.used_list.head();
        let mut reclaimed = 0;
        loop {
            if current.is_null() {
                log::debug!("Collected {} bytes.", reclaimed);
                return reclaimed;
            }
            unsafe {
                let next = (*current).next;
                if ChunkHeader::ref_count(current) == 0 {
                    let clen = (*current).len;
                    reclaimed += clen;
                    self.header.used -= clen;
                    self.header.used_list.remove(current);
                    self.header.free_list.prepend(current);
                }
                current = next;
            }
        }
    }
}

impl std::fmt::Debug for SharedMemoryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f
            .debug_struct("SharedMemoryManager")
            .field("segment_path", &self.segment_path)
            .field("size", &self.size)
            .field("offset", &self.offset)
            .field("header", &self.header)
            .finish();
        f.debug_list()
            .entries(self.segments.keys().into_iter())
            .finish()
    }
}
