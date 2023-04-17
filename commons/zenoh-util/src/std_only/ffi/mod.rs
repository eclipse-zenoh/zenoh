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
