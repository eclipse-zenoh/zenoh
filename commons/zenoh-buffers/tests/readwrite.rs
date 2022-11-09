//
// Copyright (c) 2022 ZettaScale Technology
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
use zenoh_buffers::reader::*;
use zenoh_buffers::writer::*;
use zenoh_buffers::*;

const BYTES: usize = 22;

macro_rules! run {
    ($buffer:expr) => {
        println!(">>> Write");
        let mut writer = $buffer.writer();

        writer.write_u8(0).unwrap();
        writer.write_u8(1).unwrap();

        let wbs1: [u8; 4] = [2, 3, 4, 5];
        let w = writer.write(&wbs1).unwrap();
        assert_eq!(4, w.get());

        let wbs2: [u8; 4] = [6, 7, 8, 9];
        writer.write_exact(&wbs2).unwrap();

        let wbs0: [u8; 4] = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
        let mark = writer.mark();
        writer.write_exact(&wbs0).unwrap();
        writer.rewind(mark);

        let wbs3: [u8; 4] = [10, 11, 12, 13];
        writer.write_exact(&wbs3).unwrap();

        let wbs4: [u8; 4] = [14, 15, 16, 17];
        writer
            .with_slot(4, |mut buffer| {
                let w = buffer.write(&wbs4).unwrap();
                assert_eq!(4, w.get());
                w.get()
            })
            .unwrap();

        let wbs5: [u8; 4] = [18, 19, 20, 21];
        writer
            .with_reservation::<typenum::U2, _>(|reservation, writer| {
                writer.write_exact(&wbs5[2..]).unwrap();
                let r = reservation.write::<typenum::U2>(&wbs5[..2]);
                Ok(r)
            })
            .unwrap();

        println!(">>> Read");
        let mut reader = $buffer.reader();

        let b = reader.read_u8().unwrap();
        assert_eq!(0, b);
        assert_eq!(BYTES - 1, reader.remaining());
        let b = reader.read_u8().unwrap();
        assert_eq!(1, b);
        assert_eq!(BYTES - 2, reader.remaining());

        let mut rbs: [u8; 4] = [0, 0, 0, 0];
        let r = reader.read(&mut rbs).unwrap();
        assert_eq!(4, r);
        assert_eq!(BYTES - 6, reader.remaining());
        assert_eq!(wbs1, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 10, reader.remaining());
        assert_eq!(wbs2, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 14, reader.remaining());
        assert_eq!(wbs3, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 18, reader.remaining());
        assert_eq!(wbs4, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 22, reader.remaining());
        assert_eq!(wbs5, rbs);

        match reader.read(&mut rbs) {
            Ok(bs) => assert_eq!(0, bs),
            Err(_) => {}
        }
        assert!(reader.read_u8().is_err());
        assert!(reader.read_exact(&mut rbs).is_err());
    };
}

macro_rules! run_bound {
    ($buffer:expr, $capacity:expr) => {
        println!(">>> Write bound");
        let mut writer = $buffer.writer();

        for i in 0..$capacity {
            writer.write_u8(i as u8).unwrap();
        }
        assert!(writer.write_u8(0).is_err());

        println!(">>> Read bound");
        let mut reader = $buffer.reader();

        for i in 0..$capacity {
            let j = reader.read_u8().unwrap();
            assert_eq!(i as u8, j);
        }
        assert!(reader.read_u8().is_err());
    };
}

macro_rules! run_siphon {
    ($from:expr, $fcap:expr, $into:expr, $icap:expr) => {
        println!(">>> Write siphon");
        let mut writer = $from.writer();
        for i in 0..$fcap {
            writer.write_u8(i as u8).unwrap();
        }

        println!(">>> Read siphon");
        let mut reader = $from.reader();
        let writer = $into.writer();

        let written = reader.siphon(writer).unwrap();
        assert_eq!($icap, written.get());

        let mut reader = $into.reader();
        for i in 0..$icap {
            let j = reader.read_u8().unwrap();
            assert_eq!(i as u8, j);
        }
        assert!(reader.read_u8().is_err());
    };
}

#[test]
fn buffer_slice() {
    println!("Buffer Slice");
    let mut sbuf = [0u8; BYTES];
    run!(sbuf.as_mut());
}

#[test]
fn buffer_vec() {
    println!("Buffer Vec");
    let mut vbuf = vec![];
    run!(vbuf);
}

#[test]
fn buffer_bbuf() {
    println!("Buffer BBuf");
    let capacity = 1 + u8::MAX as usize;
    let mut bbuf = BBuf::with_capacity(capacity);
    run!(bbuf);

    bbuf.clear();

    run_bound!(bbuf, capacity);
}

#[test]
fn buffer_zbuf() {
    println!("Buffer ZBuf");
    let mut zbuf = ZBuf::default();
    run!(zbuf);
}

#[test]
fn buffer_siphon() {
    let capacity = 1 + u8::MAX as usize;

    println!("Buffer Siphon BBuf({}) -> BBuf({})", capacity, capacity);
    let mut bbuf1 = BBuf::with_capacity(capacity);
    let mut bbuf2 = BBuf::with_capacity(capacity);
    run_siphon!(bbuf1, capacity, bbuf2, capacity);

    println!("Buffer Siphon ZBuf({}) -> ZBuf({})", capacity, capacity);
    let mut zbuf1 = ZBuf::default();
    let mut zbuf2 = ZBuf::default();
    run_siphon!(zbuf1, capacity, zbuf2, capacity);

    println!("Buffer Siphon ZBuf({}) -> BBuf({})", capacity, capacity);
    let mut zbuf1 = ZBuf::default();
    let mut bbuf1 = BBuf::with_capacity(capacity);
    run_siphon!(zbuf1, capacity, bbuf1, capacity);

    println!("Buffer Siphon BBuf({}) -> BBuf({})", capacity, capacity / 2);
    let mut bbuf1 = BBuf::with_capacity(capacity);
    let mut bbuf2 = BBuf::with_capacity(capacity / 2);
    run_siphon!(bbuf1, capacity, bbuf2, capacity / 2);

    println!("Buffer Siphon ZBuf({}) -> BBuf({})", capacity, capacity / 2);
    let mut zbuf1 = ZBuf::default();
    let mut bbuf1 = BBuf::with_capacity(capacity / 2);
    run_siphon!(zbuf1, capacity, bbuf1, capacity / 2);
}
