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
use std::time::Duration;
use zenoh_util::sync::Mvar;

const TIMEOUT: Duration = Duration::from_secs(60);

#[test]
fn mvar() {
    let count: usize = 1_000;
    let mvar: Arc<Mvar<usize>> = Arc::new(Mvar::new());

    let c_mvar = mvar.clone();
    let ch = task::spawn(async move {
        for _ in 0..count {
            let n = c_mvar.take().await;
            print!("-{} ", n);
        }
    });

    let c_mvar = mvar.clone();
    let ph = task::spawn(async move {
        for i in 0..count {
            c_mvar.put(i).await;
            print!("+{} ", i);
        }
    });

    task::block_on(async {
        ph.timeout(TIMEOUT).await.unwrap();
        ch.timeout(TIMEOUT).await.unwrap();
    });
    println!("");
}
