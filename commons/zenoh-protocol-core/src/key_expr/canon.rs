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
use crate::key_expr::utils::{Split, Writer};
pub trait Canonizable {
    fn canonize(&mut self);
}
impl Canonizable for &mut str {
    fn canonize(&mut self) {
        let mut writer = Writer {
            ptr: self.as_mut_ptr(),
            len: 0,
        };
        let mut ke = self.as_bytes().spliter(&b'/');
        let mut in_big_wild = false;

        for chunk in ke.by_ref() {
            if chunk.is_empty() {
                continue;
            }
            if in_big_wild {
                match chunk {
                    b"*" => {
                        writer.write_byte(b'*');
                        break;
                    }
                    b"**" => continue,
                    _ => {
                        writer.write(b"**/");
                        writer.write(chunk);
                        in_big_wild = false;
                        break;
                    }
                }
            } else if chunk == b"**" {
                in_big_wild = true;
                continue;
            } else {
                writer.write(chunk);
                break;
            }
        }
        for chunk in ke {
            if chunk.is_empty() {
                continue;
            }
            if in_big_wild {
                match chunk {
                    b"*" => {
                        writer.write(b"/*");
                    }
                    b"**" => {}
                    _ => {
                        writer.write(b"/**/");
                        writer.write(chunk);
                        in_big_wild = false;
                    }
                }
            } else if chunk == b"**" {
                in_big_wild = true;
            } else {
                writer.write(b"/");
                writer.write(chunk);
            }
        }
        if in_big_wild {
            if writer.len != 0 {
                writer.write_byte(b'/');
            }
            writer.write(b"**")
        }
        *self = unsafe {
            std::str::from_utf8_unchecked_mut(std::slice::from_raw_parts_mut(
                writer.ptr, writer.len,
            ))
        }
    }
}

impl Canonizable for String {
    fn canonize(&mut self) {
        let mut s = self.as_mut();
        s.canonize();
        let len = s.len();
        self.truncate(len);
    }
}
