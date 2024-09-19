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

use std::{borrow::Cow, collections::HashMap, io::Cursor};

use unwrap_infallible::UnwrapInfallible;
use zenoh::bytes::ZBytes;

fn main() {
    // Numeric: u8, u16, u32, u128, usize, i8, i16, i32, i128, isize, f32, f64
    let input = 1234_u32;
    let payload = ZBytes::from(input);
    let output: u32 = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_UINT32;

    // String
    let input = String::from("test");
    let payload = ZBytes::from(&input);
    let output: String = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_STRING;

    // Cow<str>
    // See [`zenoh::bytes::ZBytes`] documentation for zero-copy behaviour.
    let input = Cow::from("test");
    let payload = ZBytes::from(&input);
    let output: Cow<str> = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_STRING;

    // Vec<u8>: The deserialization should be infallible
    let input: Vec<u8> = vec![1, 2, 3, 4];
    let payload = ZBytes::from(&input);
    let output: Vec<u8> = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Deserialization of Vec<u8> is infallible. See https://docs.rs/unwrap-infallible/latest/unwrap_infallible/.
    let output: Vec<u8> = payload.deserialize().unwrap_infallible();
    assert_eq!(input, output);
    // Since the deserialization of `Vec<u8>` is infallible, then `ZBytes` can be infallibly converted into a `Vec<u8>`.
    let output: Vec<u8> = payload.into();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_BYTES;

    // Cow<[u8]>
    // See [`zenoh::bytes::ZBytes`] documentation for zero-copy behaviour.
    let input = Cow::from(vec![1, 2, 3, 4]);
    let payload = ZBytes::from(&input);
    let output: Cow<[u8]> = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Deserialization of `Cow<[u8]>` is infallible. See https://docs.rs/unwrap-infallible/latest/unwrap_infallible/.
    let output: Cow<[u8]> = payload.deserialize().unwrap_infallible();
    assert_eq!(input, output);
    // Since the deserialization of `Cow<[u8]>` is infallible, then `ZBytes` can be infallibly converted into a `Cow<[u8]>`.
    let output: Vec<u8> = payload.into();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::ZENOH_BYTES;

    // Writer & Reader
    // serialization
    let mut bytes = ZBytes::empty();
    let mut writer = bytes.writer();
    let i1 = 1234_u32;
    let i2 = String::from("test");
    let i3 = vec![1, 2, 3, 4];
    writer.serialize(i1);
    writer.serialize(&i2);
    writer.serialize(&i3);
    // deserialization
    let mut reader = bytes.reader();
    let o1: u32 = reader.deserialize().unwrap();
    let o2: String = reader.deserialize().unwrap();
    let o3: Vec<u8> = reader.deserialize().unwrap();
    assert_eq!(i1, o1);
    assert_eq!(i2, o2);
    assert_eq!(i3, o3);

    // Tuple
    let input = (1234_u32, String::from("test"));
    let payload = ZBytes::serialize(input.clone());
    let output: (u32, String) = payload.deserialize().unwrap();
    assert_eq!(input, output);

    // Iterator
    let input: [i32; 4] = [1, 2, 3, 4];
    let payload = ZBytes::from_iter(input.iter());
    for (idx, value) in payload.iter::<i32>().enumerate() {
        assert_eq!(input[idx], value.unwrap());
    }

    // Iterator RAW
    let input: [i32; 4] = [1, 2, 3, 4];
    let payload = ZBytes::from_iter(input.iter());
    for slice in payload.slices() {
        println!("{:02x?}", slice);
    }

    // HashMap
    let mut input: HashMap<usize, String> = HashMap::new();
    input.insert(0, String::from("abc"));
    input.insert(1, String::from("def"));
    let payload = ZBytes::from(input.clone());
    let output = payload.deserialize::<HashMap<usize, String>>().unwrap();
    assert_eq!(input, output);

    // JSON
    let data = r#"
    {
        "name": "John Doe",
        "age": 43,
        "phones": [
            "+44 1234567",
            "+44 2345678"
        ]
    }"#;
    let input: serde_json::Value = serde_json::from_str(data).unwrap();
    let payload = ZBytes::try_serialize(input.clone()).unwrap();
    let output: serde_json::Value = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::APPLICATION_JSON;

    // YAML
    let data = r#"
        name: "John Doe"
        age: 43
        phones:
          - "+44 1234567"
          - "+44 2345678"
    "#;
    let input: serde_yaml::Value = serde_yaml::from_str(data).unwrap();
    let payload = ZBytes::try_serialize(input.clone()).unwrap();
    let output: serde_yaml::Value = payload.deserialize().unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::APPLICATION_YAML;

    // Protobuf
    use prost::Message;
    #[derive(Message, Eq, PartialEq)]
    struct EntityInfo {
        #[prost(uint32)]
        id: u32,
        #[prost(string)]
        name: String,
    }
    let input = EntityInfo {
        id: 1234,
        name: String::from("John Doe"),
    };
    let payload = ZBytes::from(input.encode_to_vec());
    let output =
        EntityInfo::decode(Cursor::new(payload.deserialize::<Cow<[u8]>>().unwrap())).unwrap();
    assert_eq!(input, output);
    // Corresponding encoding to be used in operations like `.put()`, `.reply()`, etc.
    // let encoding = Encoding::APPLICATION_PROTOBUF;
}
