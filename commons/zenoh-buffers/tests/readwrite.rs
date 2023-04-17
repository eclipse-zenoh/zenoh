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
use std::sync::Arc;
use zenoh_buffers::reader::*;
use zenoh_buffers::writer::*;
use zenoh_buffers::*;

const BYTES: usize = 18;

const WBS0: u8 = 0;
const WBS1: u8 = 1;
const WBS2: [u8; 4] = [2, 3, 4, 5];
const WBS3: [u8; 4] = [6, 7, 8, 9];
const WBS4: [u8; 4] = [10, 11, 12, 13];
const WBS5: [u8; 4] = [14, 15, 16, 17];
const WBSN: [u8; 4] = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];

macro_rules! run_write {
    ($buffer:expr) => {
        println!(">>> Write");
        let mut writer = $buffer.writer();

        writer.write_u8(WBS0).unwrap();
        writer.write_u8(WBS1).unwrap();

        let w = writer.write(&WBS2).unwrap();
        assert_eq!(4, w.get());

        writer.write_exact(&WBS3).unwrap();

        let mark = writer.mark();
        writer.write_exact(&WBSN).unwrap();
        writer.rewind(mark);

        writer.write_exact(&WBS4).unwrap();

        writer
            .with_slot(4, |mut buffer| {
                let w = buffer.write(&WBS5).unwrap();
                assert_eq!(4, w.get());
                w.get()
            })
            .unwrap();
    };
}

macro_rules! run_read {
    ($buffer:expr) => {
        println!(">>> Read");
        let mut reader = $buffer.reader();

        let b = reader.read_u8().unwrap();
        assert_eq!(WBS0, b);
        assert_eq!(BYTES - 1, reader.remaining());
        let b = reader.read_u8().unwrap();
        assert_eq!(WBS1, b);
        assert_eq!(BYTES - 2, reader.remaining());

        let mut rbs: [u8; 4] = [0, 0, 0, 0];
        let r = reader.read(&mut rbs).unwrap();
        assert_eq!(4, r.get());
        assert_eq!(BYTES - 6, reader.remaining());
        assert_eq!(WBS2, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 10, reader.remaining());
        assert_eq!(WBS3, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 14, reader.remaining());
        assert_eq!(WBS4, rbs);

        reader.read_exact(&mut rbs).unwrap();
        assert_eq!(BYTES - 18, reader.remaining());
        assert_eq!(WBS5, rbs);

        assert!(reader.read(&mut rbs).is_err());
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

        let mut read = 0;
        while read < $fcap {
            $into.clear();

            let writer = $into.writer();
            let written = reader.siphon(writer).unwrap();

            let mut reader = $into.reader();
            for i in read..read + written.get() {
                let j = reader.read_u8().unwrap();
                assert_eq!(i as u8, j);
            }
            assert!(reader.read_u8().is_err());

            read += written.get();
        }
        assert_eq!(read, $fcap);
    };
}

#[test]
fn buffer_slice() {
    println!("Buffer Slice");
    let mut sbuf = [0u8; BYTES];
    run_write!(sbuf.as_mut());
    run_read!(sbuf.as_mut());
}

#[test]
fn buffer_vec() {
    println!("Buffer Vec");
    let mut vbuf = vec![];
    run_write!(&mut vbuf);
    run_read!(&vbuf);
}

#[test]
fn buffer_bbuf() {
    println!("Buffer BBuf");
    let capacity = 1 + u8::MAX as usize;
    let mut bbuf = BBuf::with_capacity(capacity);
    run_write!(bbuf);
    run_read!(bbuf);

    bbuf.clear();

    run_bound!(bbuf, capacity);
}

#[test]
fn buffer_zbuf() {
    println!("Buffer ZBuf");
    let mut zbuf = ZBuf::default();
    run_write!(zbuf);
    run_read!(zbuf);
}

#[test]
fn buffer_zslice() {
    println!("Buffer ZSlice");
    let mut vbuf = vec![];
    run_write!(&mut vbuf);

    let mut zslice = ZSlice::from(Arc::new(vbuf));
    run_read!(zslice);
}

#[test]
fn buffer_siphon() {
    let capacity = 1 + u8::MAX as usize;

    println!("Buffer Siphon BBuf({capacity}) -> BBuf({capacity})");
    let mut bbuf1 = BBuf::with_capacity(capacity);
    let mut bbuf2 = BBuf::with_capacity(capacity);
    run_siphon!(bbuf1, capacity, bbuf2, capacity);

    println!("Buffer Siphon ZBuf({capacity}) -> ZBuf({capacity})");
    let mut zbuf1 = ZBuf::default();
    let mut zbuf2 = ZBuf::default();
    run_siphon!(zbuf1, capacity, zbuf2, capacity);

    println!("Buffer Siphon ZBuf({capacity}) -> BBuf({capacity})");
    let mut zbuf1 = ZBuf::default();
    let mut bbuf1 = BBuf::with_capacity(capacity);
    run_siphon!(zbuf1, capacity, bbuf1, capacity);

    let capacity2 = 1 + capacity / 2;
    println!("Buffer Siphon BBuf({capacity}) -> BBuf({capacity2})");
    let mut bbuf1 = BBuf::with_capacity(capacity);
    let mut bbuf2 = BBuf::with_capacity(capacity2);
    run_siphon!(bbuf1, capacity, bbuf2, capacity2);

    println!("Buffer Siphon ZBuf({capacity}) -> BBuf({capacity2})");
    let mut zbuf1 = ZBuf::default();
    let mut bbuf1 = BBuf::with_capacity(capacity2);
    run_siphon!(zbuf1, capacity, bbuf1, capacity2);
}
