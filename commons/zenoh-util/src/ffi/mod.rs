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
#[cfg(windows)]
pub mod win;

/// # Safety
/// Dereferences raw pointer argument
pub unsafe fn pwstr_to_string(ptr: *mut u16) -> String {
    use std::slice::from_raw_parts;
    let len = (0_usize..)
        .find(|&n| *ptr.add(n) == 0)
        .expect("Couldn't find null terminator");
    let array: &[u16] = from_raw_parts(ptr, len);
    String::from_utf16_lossy(array)
}

/// # Safety
/// Dereferences raw pointer argument
pub unsafe fn pstr_to_string(ptr: *mut i8) -> String {
    use std::slice::from_raw_parts;
    let len = (0_usize..)
        .find(|&n| *ptr.add(n) == 0)
        .expect("Couldn't find null terminator");
    let array: &[u8] = from_raw_parts(ptr as *const u8, len);
    String::from_utf8_lossy(array).to_string()
}

/// Struct used to safely exchange data in json format in plugins.
/// It is not entirely abi stable due to using String, but should do
/// for now since we require plugins to use the same version of rustc and
/// same version of zenoh.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(from = "serde_json::Value")]
#[serde(into = "serde_json::Value")]
pub struct JsonValue(String);

impl PartialEq for JsonValue {
    fn eq(&self, other: &Self) -> bool {
        let left: serde_json::Value = self.into();
        let right: serde_json::Value = other.into();
        left == right
    }
}

impl Eq for JsonValue {}

impl From<&serde_json::Value> for JsonValue {
    fn from(value: &serde_json::Value) -> Self {
        JsonValue(serde_json::to_string(value).unwrap())
    }
}

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        (&value).into()
    }
}

impl From<&JsonValue> for serde_json::Value {
    fn from(value: &JsonValue) -> Self {
        serde_json::from_str(&value.0).unwrap()
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(value: JsonValue) -> Self {
        (&value).into()
    }
}

impl Default for JsonValue {
    fn default() -> Self {
        serde_json::Value::default().into()
    }
}

impl JsonValue {
    pub fn into_serde_value(&self) -> serde_json::Value {
        self.into()
    }
}

/// Struct used to safely exchange data in json format in plugins.
/// It is not entirely abi stable due to using String, but should do
/// for now since we require plugins to use the same version of rustc and
/// same version of zenoh.
#[derive(Debug, Clone)]
pub struct JsonKeyValueMap(String);

impl From<&serde_json::Map<String, serde_json::Value>> for JsonKeyValueMap {
    fn from(value: &serde_json::Map<String, serde_json::Value>) -> Self {
        JsonKeyValueMap(serde_json::to_string(value).unwrap())
    }
}

impl From<serde_json::Map<String, serde_json::Value>> for JsonKeyValueMap {
    fn from(value: serde_json::Map<String, serde_json::Value>) -> Self {
        (&value).into()
    }
}

impl From<&JsonKeyValueMap> for serde_json::Map<String, serde_json::Value> {
    fn from(value: &JsonKeyValueMap) -> Self {
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&value.0).unwrap()
    }
}

impl From<JsonKeyValueMap> for serde_json::Map<String, serde_json::Value> {
    fn from(value: JsonKeyValueMap) -> Self {
        (&value).into()
    }
}

impl Default for JsonKeyValueMap {
    fn default() -> Self {
        serde_json::Map::<String, serde_json::Value>::default().into()
    }
}

impl PartialEq for JsonKeyValueMap {
    fn eq(&self, other: &Self) -> bool {
        let left: serde_json::Map<String, serde_json::Value> = self.into();
        let right: serde_json::Map<String, serde_json::Value> = other.into();
        left == right
    }
}

impl Eq for JsonKeyValueMap {}

impl JsonKeyValueMap {
    pub fn into_serde_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.into()
    }
}
