//
// Copyright (c) 2026 ZettaScale Technology
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

use std::sync::Arc;

use zenoh_result::ZResult;

use crate::api::reader::{fragmented_batch::FragmentedBatch, rx_buffer::RxBuffer};

#[derive(Debug)]
struct BatchAccumulator {
    accumulated_size: usize,
    pub(crate) batch: FragmentedBatch,
}

impl BatchAccumulator {
    fn new(accumulated_size: usize, batch: FragmentedBatch) -> Self {
        Self {
            accumulated_size,
            batch,
        }
    }
}

#[derive(Debug)]
enum RxWindowState {
    Initial,
    SizeFragmented(u8),
    Accumulating(BatchAccumulator),
}

#[derive(Debug)]
pub struct RxWindow {
    state: RxWindowState,
}

impl Default for RxWindow {
    fn default() -> Self {
        Self {
            state: RxWindowState::Initial,
        }
    }
}

//macro_rules! log {
//    ($level:expr, $($arg:tt)*) => {
//        eprintln!("[{}] {}:{} - {}",
//            $level,
//            file!(),
//            line!(),
//            format_args!($($arg)*));
//    };
//}

impl RxWindow {
    pub(crate) fn push<F>(&mut self, buffer: Arc<RxBuffer>, on_batch: &mut F) -> ZResult<()>
    where
        F: FnMut(FragmentedBatch) -> ZResult<()>,
    {
        #[cfg(feature = "uring_trace")]
        tracing::trace!("Buffer len: {}", buffer.len());

        fn parse_size(bytes: [u8; 2]) -> usize {
            let size = u16::from_le_bytes(bytes) as usize;
            #[cfg(feature = "uring_trace")]
            tracing::trace!("parsed size: {}", size);
            size
        }

        match &mut self.state {
            RxWindowState::Initial => {
                let mut leftover = buffer.len();
                let mut pos = 0;

                while leftover > 0 {
                    // buffer contains size fragment
                    if leftover == 1 {
                        self.state = RxWindowState::SizeFragmented(buffer[pos]);
                        break;
                    }

                    // size may be 0!
                    let size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
                    pos += 2;
                    leftover -= 2;

                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch::new(size, 0, vec![]);

                        if size == 0 {
                            self.state = RxWindowState::Initial;
                            on_batch(fragment)?;
                        } else {
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                        }
                        break;
                    }

                    // buffer contains batch fragment
                    if leftover < size {
                        let fragment = FragmentedBatch::new(size, pos, vec![buffer]);
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        assert!(size != 0);
                        break;
                    }

                    // don't borrow buffer for ephemeral batches
                    let buffers = if size == 0 {
                        vec![]
                    } else {
                        vec![buffer.clone()]
                    };

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch::new(size, pos, buffers);
                    pos += size;
                    leftover -= size;
                    #[cfg(feature = "uring_trace")]
                    tracing::trace!("on_batch");
                    on_batch(batch)?;
                }
            }
            RxWindowState::SizeFragmented(size_fragment) => {
                // defragment size
                // size may be 0!
                let mut size = parse_size([*size_fragment, buffer[0]]);
                let mut leftover = buffer.len() - 1;
                let mut pos = 1;

                loop {
                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch::new(size, 0, vec![]);

                        if size == 0 {
                            self.state = RxWindowState::Initial;
                            on_batch(fragment)?;
                        } else {
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                        }
                        break;
                    }

                    // buffer contains batch fragment
                    if leftover < size {
                        let fragment = FragmentedBatch::new(size, pos, vec![buffer]);
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        assert!(size != 0);
                        break;
                    }

                    // don't borrow buffer for ephemeral batches
                    let buffers = if size == 0 {
                        vec![]
                    } else {
                        vec![buffer.clone()]
                    };

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch::new(size, pos, buffers);
                    pos += size;
                    leftover -= size;
                    #[cfg(feature = "uring_trace")]
                    tracing::trace!("on_batch");
                    on_batch(batch)?;

                    // buffer ends with some exact number of batches
                    if leftover == 0 {
                        self.state = RxWindowState::Initial;
                        break;
                    }

                    // buffer contains size fragment
                    if leftover == 1 {
                        self.state = RxWindowState::SizeFragmented(buffer[pos]);
                        break;
                    }

                    // size may be 0!
                    size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
                    pos += 2;
                    leftover -= 2;
                }
            }
            RxWindowState::Accumulating(batch_accumulator) => {
                batch_accumulator.accumulated_size += buffer.len();
                batch_accumulator.batch.buffers.push(buffer.clone());

                if batch_accumulator.accumulated_size >= batch_accumulator.batch.size {
                    let mut leftover =
                        batch_accumulator.accumulated_size - batch_accumulator.batch.size;
                    let mut pos = buffer.len() - leftover;

                    {
                        // send fragmented batch
                        let mut batch = FragmentedBatch {
                            size: batch_accumulator.batch.size,
                            data_offset: batch_accumulator.batch.data_offset,
                            buffers: vec![], //  batch_accumulator.batch.buffers.clone(),
                        };
                        std::mem::swap(&mut batch.buffers, &mut batch_accumulator.batch.buffers);
                        #[cfg(feature = "uring_trace")]
                        tracing::trace!("on_batch");
                        on_batch(batch)?;
                    }

                    loop {
                        // no more data
                        if leftover == 0 {
                            self.state = RxWindowState::Initial;
                            #[cfg(feature = "uring_trace")]
                            tracing::trace!("Accumulating -> Initial");
                            break;
                        }

                        // buffer contains size fragment
                        if leftover == 1 {
                            self.state = RxWindowState::SizeFragmented(buffer[pos]);
                            #[cfg(feature = "uring_trace")]
                            tracing::trace!("Accumulating -> SizeFragmented");
                            break;
                        }

                        // size may be 0!
                        let size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
                        pos += 2;
                        leftover -= 2;

                        // only size
                        if leftover == 0 {
                            let fragment = FragmentedBatch::new(size, 0, vec![]);

                            if size == 0 {
                                self.state = RxWindowState::Initial;
                                #[cfg(feature = "uring_trace")]
                                tracing::trace!("Accumulating -> Initial");
                                on_batch(fragment)?;
                            } else {
                                let accumulator = BatchAccumulator::new(leftover, fragment);
                                self.state = RxWindowState::Accumulating(accumulator);
                                #[cfg(feature = "uring_trace")]
                                tracing::trace!("Accumulating -> Accumulating (only size)");
                            }
                            break;
                        }

                        // buffer contains batch fragment
                        if leftover < size {
                            let fragment = FragmentedBatch::new(size, pos, vec![buffer]);
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                            assert!(size != 0);
                            #[cfg(feature = "uring_trace")]
                            tracing::trace!("Accumulating -> Accumulating (fragment)");
                            break;
                        }

                        // don't borrow buffer for ephemeral batches
                        let buffers = if size == 0 {
                            vec![]
                        } else {
                            vec![buffer.clone()]
                        };

                        // buffer contains at least one more batch
                        let batch = FragmentedBatch::new(size, pos, buffers);
                        pos += size;
                        leftover -= size;
                        #[cfg(feature = "uring_trace")]
                        tracing::trace!("on_batch");
                        on_batch(batch)?;
                    }
                }
            }
        }
        Ok(())
    }
}
