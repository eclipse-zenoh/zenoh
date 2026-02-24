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

use std::cmp::max;

use zenoh_uring::reader::Reader;

#[derive(Clone)]
pub struct Uring {
    //pub writer: Arc<Writer>,
    pub reader: Reader,
}

impl Uring {
    pub fn new(batch_size: usize, link_rx_buffer_size: usize) -> Self {
        // add 2 bytes for size header in case of streamed links
        let batch_size = batch_size + (u16::BITS / 8) as usize;
        let batch_count = max(link_rx_buffer_size / batch_size, 2);

        //let writer = Arc::new(Writer::new());
        let reader = Reader::new(batch_size, batch_count);
        Self {
            /*writer,*/ reader,
        }
    }
}
