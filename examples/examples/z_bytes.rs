//
// Copyright (c) 2024 ZettaScale Technology
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
use std::collections::HashMap;

use zenoh::bytes::ZBytes;

fn main() {
    // Raw bytes
    let input = b"raw bytes".as_slice();
    // raw bytes are copied into ZBytes, or moved in case of Vec<u8>
    let payload_copy = ZBytes::from(input);
    let payload_move = ZBytes::from(input.to_vec());
    assert_eq!(payload_copy, payload_move);
    // retrieving raw bytes from ZBytes is infallible
    let output = payload_move.to_bytes();
    assert_eq!(input, &*output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_BYTES;

    // Raw utf8 bytes, i.e. string
    let input = "raw bytes";
    // string is copied into ZBytes, or moved in case of String
    let payload_copy = ZBytes::from(input);
    let payload_move = ZBytes::from(input.to_string());
    assert_eq!(payload_copy, payload_move);
    // retrieving utf8 string from ZBytes can fail if the bytes are not utf8
    let output = payload_move.try_to_string().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_STRING;

    // JSON
    let input = serde_json::json!({
        "name": "John Doe",
        "age": 43,
        "phones": [
            "+44 1234567",
            "+44 2345678"
        ]
    });
    let payload = ZBytes::from(serde_json::to_vec(&input).unwrap());
    let output: serde_json::Value = serde_json::from_slice(&payload.to_bytes()).unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::APPLICATION_JSON;

    // Protobuf (see example.proto)
    mod example {
        include!(concat!(env!("OUT_DIR"), "/example.rs"));
    }
    use prost::Message;
    let input = example::Entity {
        id: 1234,
        name: String::from("John Doe"),
    };
    let payload = ZBytes::from(input.encode_to_vec());
    let output = example::Entity::decode(&*payload.to_bytes()).unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::APPLICATION_PROTOBUF;

    // zenoh-ext serialization
    {
        use zenoh_ext::{z_deserialize, z_serialize};

        // Numeric types: u8, u16, u32, u128, i8, i16, i32, i128, f32, f64
        let input = 1234_u32;
        let payload = z_serialize(&input);
        let output: u32 = z_deserialize(&payload).unwrap();
        assert_eq!(input, output);

        // Vec
        let input = vec![0.0f32, 1.5, 42.0];
        let payload = z_serialize(&input);
        let output: Vec<f32> = z_deserialize(&payload).unwrap();
        assert_eq!(input, output);

        // HashMap
        let mut input: HashMap<u32, String> = HashMap::new();
        input.insert(0, String::from("abc"));
        input.insert(1, String::from("def"));
        let payload = z_serialize(&input);
        let output: HashMap<u32, String> = z_deserialize(&payload).unwrap();
        assert_eq!(input, output);

        // Tuple
        let input = (0.42f64, "string".to_string());
        let payload = z_serialize(&input);
        let output: (f64, String) = z_deserialize(&payload).unwrap();
        assert_eq!(input, output);

        // Array (handled as variable-length sequence, not as tuple)
        let input = [0.0f32, 1.5, 42.0];
        let payload = z_serialize(&input);
        let output: [f32; 3] = z_deserialize(&payload).unwrap();
        assert_eq!(input, output);
        // can also be deserialized as a vec
        let output: Vec<f32> = z_deserialize(&payload).unwrap();
        assert_eq!(input.as_slice(), output);

        // Look at Serialize/Deserialize documentation for the exhaustive
        // list of provided implementations
    }

    // Writer/reader
    use std::io::{Read, Write};
    let input1 = &[0u8, 1];
    let input2 = ZBytes::from([2, 3]);
    let mut writer = ZBytes::writer();
    writer.write_all(&[0u8, 1]).unwrap();
    writer.append(input2.clone());
    let zbytes = writer.finish();
    assert_eq!(*zbytes.to_bytes(), [0u8, 1, 2, 3]);
    let mut reader = zbytes.reader();
    let mut buf = [0; 2];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf, *input1);
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf, *input2.to_bytes());
}
