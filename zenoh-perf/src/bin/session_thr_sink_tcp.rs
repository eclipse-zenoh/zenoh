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
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use rand::RngCore;
use std::convert::TryInto;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh_protocol::core::{whatami, PeerId};
use zenoh_protocol::io::{RBuf, WBuf};
use zenoh_protocol::proto::{OpenSyn, SessionBody, SessionMessage};

macro_rules! zsend {
    ($msg:expr, $stream:expr) => {{
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(32, false);
        // Reserve 16 bits to write the length
        assert!(wbuf.write_bytes(&[0u8, 0u8]));
        // Serialize the message
        assert!(wbuf.write_session_message(&$msg));
        // Write the length on the first 16 bits
        let length: u16 = wbuf.len() as u16 - 2;
        let bits = wbuf.get_first_slice_mut(..2);
        bits.copy_from_slice(&length.to_le_bytes());
        let mut buffer = vec![0u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        let res = $stream.write_all(&buffer).await;
        log::trace!("Sending {:?}: {:?}", $msg, res);

        res
    }};
}

macro_rules! zrecv {
    ($stream:expr, $buffer:expr) => {{
        let _ = $stream.read_exact(&mut $buffer[0..2]).await.unwrap();
        let length: [u8; 2] = $buffer[0..2].try_into().unwrap();
        // Decode the total amount of bytes that we are expected to read
        let to_read = u16::from_le_bytes(length) as usize;
        $stream.read_exact(&mut $buffer[0..to_read]).await.unwrap();
        let mut rbuf = RBuf::from(&$buffer[..]);
        rbuf.read_session_message().unwrap()
    }};
}

async fn handle_client(mut stream: TcpStream, bs: usize) -> Result<(), Box<dyn std::error::Error>> {
    let my_whatami = whatami::ROUTER;
    let mut my_pid = [0u8; PeerId::MAX_SIZE];
    rand::thread_rng().fill_bytes(&mut my_pid);
    let my_pid = PeerId::new(1, my_pid);

    // Create the reading buffer
    let mut buffer = vec![0u8; bs];

    // Read the InitSyn
    let message = zrecv!(stream, buffer);
    match message.get_body() {
        SessionBody::InitSyn { .. } => {
            let whatami = my_whatami;
            let sn_resolution = None;
            let cookie = RBuf::from(vec![0u8; 8]);
            let attachment = None;
            let message = SessionMessage::make_init_ack(
                whatami,
                my_pid.clone(),
                sn_resolution,
                cookie,
                attachment,
            );

            let _ = zsend!(message, stream).unwrap();
        }
        _ => panic!(),
    }

    // Read the OpenSyn
    let message = zrecv!(stream, buffer);
    match message.get_body() {
        SessionBody::OpenSyn(OpenSyn {
            lease, initial_sn, ..
        }) => {
            let attachment = None;
            let message = SessionMessage::make_open_ack(*lease, *initial_sn, attachment);

            let _ = zsend!(message, stream).unwrap();
        }
        _ => panic!(),
    }

    let counter = Arc::new(AtomicUsize::new(0));
    let c_c = counter.clone();
    task::spawn(async move {
        loop {
            task::sleep(Duration::from_secs(1)).await;
            let c = c_c.swap(0, Ordering::Relaxed);
            println!("{:.3} Gbit/s", (8_f64 * c as f64) / 1000000000_f64);
        }
    });

    let mut c_stream = stream.clone();
    task::spawn(async move {
        loop {
            task::sleep(Duration::from_secs(1)).await;
            let message = SessionMessage::make_keep_alive(None, None);

            let _ = zsend!(message, c_stream).unwrap();
        }
    });

    loop {
        let n = stream.read(&mut buffer).await.unwrap();
        counter.fetch_add(n, Ordering::AcqRel);
    }
}

async fn run(addr: SocketAddr, bs: usize) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        task::spawn(async move {
            let _ = handle_client(stream, bs).await;
        });
    }

    Ok(())
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <TCP address to listen on> <buffer size>
Example:
    cargo run --release --bin {} 127.0.0.1:7447 65537",
        bin, bin
    );
}

fn main() {
    // Enable logging
    env_logger::init();

    let mut args = std::env::args();
    // Get exe name
    let bin = args
        .next()
        .unwrap()
        .split(std::path::MAIN_SEPARATOR)
        .last()
        .unwrap()
        .to_string();

    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let addr: SocketAddr = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let bs: usize = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    let _ = task::block_on(run(addr, bs));
}
