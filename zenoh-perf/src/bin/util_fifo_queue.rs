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
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use std::time::Instant;

use zenoh_util::collections::FifoQueue;

fn main() {
    task::block_on(async {
        let acb = Arc::new(FifoQueue::<u64>::new(256, 16));
        let cq1 = acb.clone();
        let cq2 = acb.clone();
        let cq3 = acb.clone();
        let cq4 = acb.clone();
        let cq5 = acb.clone();
        let cq6 = acb.clone();
        let now = Instant::now();
        let n = 1_000_000u32;

        let p1 = task::spawn(async move {
            for i in 0..n {
                cq1.push(i as u64).await;
            }
        });

        let p2 = task::spawn(async move {
            for i in 0..n {
                cq2.push(i as u64).await;
            }
        });

        let p3 = task::spawn(async move {
            for i in 0..n {
                cq3.push(i as u64).await;
            }
        });
        let p4 = task::spawn(async move {
            for i in 0..n {
                cq4.push(i as u64).await;
            }
        });

        let c1 = task::spawn(async move {
            for _ in 0..(2 * n) {
                let _ = cq5.pull().await;
            }
        });

        let c2 = task::spawn(async move {
            for _ in 0..(2 * n) {
                let _ = cq6.pull().await;
            }
        });

        p1.join(p2).join(p3).join(p4).join(c1).join(c2).await;
        println!("Test run in: {}", now.elapsed().as_millis());
    });
}
