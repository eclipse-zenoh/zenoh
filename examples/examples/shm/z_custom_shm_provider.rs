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
    alloc::Layout,
    num::NonZeroUsize,
    ptr::NonNull,
    slice,
    sync::{Arc, Mutex},
    time::Duration,
};

use talc::{ErrOnOom, Talc};
use zenoh::{
    shm::{
        AllocAlignment, AllocatedChunk, BlockOn, ChunkAllocResult, ChunkDescriptor, ChunkID,
        GarbageCollect, MemoryLayout, ProtocolID, PtrInSegment, SegmentID, ShmClient,
        ShmClientStorage, ShmProviderBackend, ShmProviderBuilder, ShmSegment, WithProtocolID,
        ZAllocError, ZLayoutError,
    },
    Config, Wait,
};
use zenoh_examples::shm::print_sample_info;
use zenoh_result::ZResult;
use zenoh_shm::posix_shm::array::ArrayInSHM;

/// Protocol identifier to use when creating ShmProvider
pub const EXAMPLE_PROTOCOL_ID: ProtocolID = 9999;

#[derive(Debug)]
pub(crate) struct PosixShmSegment {
    segment: ArrayInSHM<SegmentID, u8, ChunkID>,
}

impl PosixShmSegment {
    pub(crate) fn create(alloc_size: NonZeroUsize) -> ZResult<Self> {
        let segment = ArrayInSHM::create(alloc_size)?;
        Ok(Self { segment })
    }

    pub(crate) fn open(id: SegmentID) -> ZResult<Self> {
        let segment = ArrayInSHM::open(id)?;
        Ok(Self { segment })
    }

    pub(crate) fn allocated_chunk(
        self: Arc<Self>,
        buf: NonNull<u8>,
        layout: &MemoryLayout,
    ) -> AllocatedChunk {
        AllocatedChunk {
            descriptor: ChunkDescriptor::new(
                self.segment.id(),
                unsafe { self.segment.index(buf.as_ptr()) },
                layout.size(),
            ),
            data: PtrInSegment::new(buf.as_ptr(), self),
        }
    }
}

impl ShmSegment for PosixShmSegment {
    fn map(&self, chunk: ChunkID) -> ZResult<*mut u8> {
        Ok(unsafe { self.segment.elem_mut(chunk) })
    }
}

/// Client factory implementation for our shared memory protocol
#[derive(Debug)]
pub struct ExampleShmClient;

impl WithProtocolID for ExampleShmClient {
    fn id(&self) -> ProtocolID {
        EXAMPLE_PROTOCOL_ID
    }
}

impl ShmClient for ExampleShmClient {
    /// Attach to particular shared memory segment
    fn attach(&self, segment: SegmentID) -> ZResult<Arc<dyn ShmSegment>> {
        Ok(Arc::new(PosixShmSegment::open(segment)?))
    }
}

/// An example backend based on POSIX shared memory.
pub struct ExampleShmProviderBackend {
    segment: Arc<PosixShmSegment>,
    talc: Mutex<Talc<ErrOnOom>>,
    alignment: AllocAlignment,
}

impl ExampleShmProviderBackend {
    pub fn new(layout: &MemoryLayout) -> ZResult<Self> {
        let segment = Arc::new(PosixShmSegment::create(layout.size())?);

        let talc = {
            let ptr = unsafe { segment.segment.elem_mut(0) };
            let mut talc = Talc::new(ErrOnOom);
            unsafe {
                talc.claim(slice::from_raw_parts_mut(ptr, layout.size().get()).into())
                    .map_err(|_| "Error initializing Talc backend!")?;
            }
            talc
        };

        println!(
            "Created PosixShmProviderBackendTalc id {}, layout {:?}",
            segment.segment.id(),
            layout
        );

        Ok(Self {
            segment,
            talc: Mutex::new(talc),
            alignment: layout.alignment(),
        })
    }
}

impl WithProtocolID for ExampleShmProviderBackend {
    fn id(&self) -> ProtocolID {
        EXAMPLE_PROTOCOL_ID
    }
}

impl ShmProviderBackend for ExampleShmProviderBackend {
    fn alloc(&self, layout: &MemoryLayout) -> ChunkAllocResult {
        let alloc = {
            let mut lock = self.talc.lock().unwrap();
            unsafe { lock.malloc(layout.into()) }
        };

        match alloc {
            Ok(buf) => Ok(self.segment.clone().allocated_chunk(buf, layout)),
            Err(_) => Err(ZAllocError::OutOfMemory),
        }
    }

    fn free(&self, chunk: &ChunkDescriptor) {
        let alloc_layout = unsafe {
            Layout::from_size_align_unchecked(
                chunk.len.get(),
                self.alignment.get_alignment_value().get(),
            )
        };

        let ptr = unsafe { self.segment.segment.elem_mut(chunk.chunk) };

        unsafe {
            self.talc
                .lock()
                .unwrap()
                .free(NonNull::new_unchecked(ptr), alloc_layout)
        };
    }

    fn defragment(&self) -> usize {
        0
    }

    fn available(&self) -> usize {
        0
    }

    fn layout_for(&self, layout: MemoryLayout) -> Result<MemoryLayout, ZLayoutError> {
        layout.extend(self.alignment)
    }
}

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let key_expr = "examples/custom_shm_provider";

    tokio::task::spawn(publisher_session(key_expr));
    tokio::task::spawn(subscriber_session(key_expr));

    println!("Press CTRL-C to quit...");
    tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
}

async fn publisher_session(key_expr: &'static str) {
    let layout: MemoryLayout = 4096.try_into().unwrap();

    println!("Creating example SHM provider...");

    let backend = ExampleShmProviderBackend::new(&layout).unwrap();
    let provider = ShmProviderBuilder::backend(backend).wait();

    println!("Opening session...");
    let session = zenoh::open(Config::default()).await.unwrap();

    println!("Declaring Publisher on '{}'...", key_expr);
    let publisher = session.declare_publisher(key_expr).await.unwrap();

    let payload = "SHM";
    let mut idx = 0u64;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        // We reserve a small space at the beginning of the buffer to include the iteration index
        // of the write. This is simply to have the same format as zn_pub.
        let prefix = format!("[{idx:4}] ");
        let prefix_len = prefix.len();
        let slice_len = prefix_len + payload.len();

        let mut buffer = provider
            .alloc(slice_len)
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .unwrap();

        buffer[0..prefix_len].copy_from_slice(prefix.as_bytes());
        buffer[prefix_len..slice_len].copy_from_slice(payload.as_bytes());

        // Write the data
        println!(
            "Put SHM Data ('{}': '{}')",
            key_expr,
            String::from_utf8_lossy(&buffer[0..slice_len])
        );
        publisher.put(buffer).await.unwrap();

        idx = idx + 1;
    }
}

async fn subscriber_session(key_expr: &'static str) {
    let custom_shm_client_storage = Arc::new(
        ShmClientStorage::builder()
            .with_client(Arc::new(ExampleShmClient))
            .build(),
    );

    println!("Opening session...");
    let session = zenoh::open(Config::default())
        .with_shm_clients(custom_shm_client_storage)
        .await
        .unwrap();

    println!("Declaring Subscriber on '{}'...", key_expr);
    let _subscriber = session
        .declare_subscriber(key_expr)
        .callback(print_sample_info)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
}
