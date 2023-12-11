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

use clap::Parser;
use std::convert::TryInto;
use zenoh::prelude::sync::*;
use zenoh::publication::CongestionControl;
use zenoh::sample::AttachmentBuilder;
use zenoh_examples::CommonArgs;

fn main() {
    // initiate logging
    env_logger::init();
    let args = Args::parse();

    let mut prio = Priority::default();
    if let Some(p) = args.priority {
        prio = p.try_into().unwrap();
    }

    let mut payload_size = args.payload_size;
    if args.attachments_number != 0 {
        let mut att_size = 2 * args.attachments_number;
        att_size += 2 + (core::mem::size_of::<usize>() * 8 - att_size.leading_zeros() as usize) / 7;
        payload_size -= dbg!(att_size);
    }

    let data: Value = (0usize..dbg!(payload_size))
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    let session = zenoh::open(args.common).res().unwrap();

    let publisher = session
        .declare_publisher("test/thr")
        .congestion_control(CongestionControl::Block)
        .priority(prio)
        .res()
        .unwrap();

    let mut count: usize = 0;
    let mut start = std::time::Instant::now();
    loop {
        let attachments = (args.attachments_number != 0).then(|| {
            if args.attach_with_insert {
                let mut attachments = AttachmentBuilder::new();
                for _ in 0..args.attachments_number {
                    attachments.insert(b"", b"");
                }
                attachments.into()
            } else {
                std::iter::repeat((b"".as_slice(), b"".as_slice()))
                    .take(args.attachments_number)
                    .collect()
            }
        });

        let mut put = publisher.put(data.clone());
        if let Some(att) = attachments {
            put = put.with_attachment(att)
        }
        put.res().unwrap();

        if args.print {
            if count < args.number {
                count += 1;
            } else {
                let thpt = count as f64 / start.elapsed().as_secs_f64();
                println!("{thpt} msg/s");
                count = 0;
                start = std::time::Instant::now();
            }
        }
    }
}

#[derive(Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    /// Priority for sending data
    #[arg(short, long)]
    priority: Option<u8>,
    /// Print the statistics
    #[arg(short = 't', long)]
    print: bool,
    /// Number of messages in each throughput measurements
    #[arg(short, long, default_value = "100000")]
    number: usize,
    /// The number of attachments in the message.
    ///
    /// The attachments will be sized such that the attachments replace the payload.
    #[arg(long, default_value = "0")]
    attachments_number: usize,
    /// Attach through insert rather than FromIterator
    #[arg(long)]
    attach_with_insert: bool,
    /// Sets the size of the payload to publish
    payload_size: usize,
    #[command(flatten)]
    common: CommonArgs,
}
