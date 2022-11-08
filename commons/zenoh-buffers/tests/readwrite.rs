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
        assert_eq!(4, w);

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
                assert_eq!(4, w);
                w
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
    let mut bbuf = BBuf::with_capacity(u8::MAX as usize);
    run!(bbuf);

    bbuf.clear();
    let mut writer = bbuf.writer();

    writer.write_exact(&vec![0u8; u8::MAX as usize]).unwrap();
    assert!(writer
        .write_exact(&vec![0u8; 1 + u8::MAX as usize])
        .is_err());
}

#[test]
fn buffer_zbuf() {
    println!("Buffer ZBuf");
    let mut zbuf = ZBuf::default();
    run!(zbuf);
}
