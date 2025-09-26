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
use alloc::string::String;

/// Helper trait implemented for types that can be canonized in place
/// by `KeyExpr::autocanonize()`.
pub trait Canonize {
    fn canonize(&mut self);
}

// Return the length of the canonized string
fn canonize(bytes: &mut [u8]) -> usize {
    let mut index = 0;
    let mut written = 0;
    let mut double_wild = false;
    loop {
        match &bytes[index..] {
            [b'*', b'*'] => {
                bytes[written..written + 2].copy_from_slice(b"**");
                written += 2;
                return written;
            }
            [b'*', b'*', b'/', ..] => {
                double_wild = true;
                index += 3;
            }
            [b'*', r @ ..] | [b'$', b'*', r @ ..] if r.is_empty() || r.starts_with(b"/") => {
                let (end, len) = (!r.starts_with(b"/"), r.len());
                bytes[written] = b'*';
                written += 1;
                if end {
                    if double_wild {
                        bytes[written..written + 3].copy_from_slice(b"/**");
                        written += 3;
                    }
                    return written;
                }
                bytes[written] = b'/';
                written += 1;
                index = bytes.len() - len + 1;
            }
            // Handle chunks with only repeated "$*"
            [b'$', b'*', b'$', b'*', ..] => {
                index += 2;
            }
            _ => {
                if double_wild && &bytes[index..] != b"**" {
                    bytes[written..written + 3].copy_from_slice(b"**/");
                    written += 3;
                    double_wild = false;
                }
                let mut write_start = index;
                loop {
                    match bytes.get(index) {
                        Some(b'/') => {
                            index += 1;
                            bytes.copy_within(write_start..index, written);
                            written += index - write_start;
                            break;
                        }
                        Some(b'$') if matches!(bytes.get(index + 1..index + 4), Some(b"*$*")) => {
                            index += 2;
                            bytes.copy_within(write_start..index, written);
                            written += index - write_start;
                            let skip = bytes[index + 4..]
                                .windows(2)
                                .take_while(|s| s == b"$*")
                                .count();
                            index += (1 + skip) * 2;
                            write_start = index;
                        }
                        Some(_) => index += 1,
                        None => {
                            bytes.copy_within(write_start..index, written);
                            written += index - write_start;
                            return written;
                        }
                    }
                }
            }
        }
    }
}

impl Canonize for &mut str {
    fn canonize(&mut self) {
        // SAFETY: canonize leave an UTF8 string within the returned length,
        // and remaining garbage bytes are zeroed
        let bytes = unsafe { self.as_bytes_mut() };
        let length = canonize(bytes);
        bytes[length..].fill(b'\0');
        *self = &mut core::mem::take(self)[..length];
    }
}

impl Canonize for String {
    fn canonize(&mut self) {
        // SAFETY: canonize leave an UTF8 string within the returned length,
        // and remaining garbage bytes are truncated
        let bytes = unsafe { self.as_mut_vec() };
        let length = canonize(bytes);
        bytes.truncate(length);
    }
}

#[test]
fn canonizer() {
    use super::OwnedKeyExpr;

    dbg!(OwnedKeyExpr::autocanonize(String::from("/a/b/")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("/a/b")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/*$*")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/$**")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/**$*")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/*$**")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/*$***")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/**$**")).unwrap_err());
    dbg!(OwnedKeyExpr::autocanonize(String::from("a/b/**$***")).unwrap_err());

    //
    // Check statements declared in https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md
    //
    // Any contiguous sequence of $*s is replaced by a single $*
    let mut s = String::from("hello/foo$*$*/bar");
    s.canonize();
    assert_eq!(s, "hello/foo$*/bar");

    // Any contiguous sequence of ** chunks is replaced by a single ** chunk
    let mut s = String::from("hello/**/**/bye");
    s.canonize();
    assert_eq!(s, "hello/**/bye");
    let mut s = String::from("hello/**/**");
    s.canonize();
    assert_eq!(s, "hello/**");

    // Any $* chunk is replaced by a * chunk
    let mut s = String::from("hello/$*/bye");
    s.canonize();
    assert_eq!(s, "hello/*/bye");
    let mut s = String::from("hello/$*$*/bye");
    s.canonize();
    assert_eq!(s, "hello/*/bye");
    let mut s = String::from("$*/hello/$*/bye");
    s.canonize();
    assert_eq!(s, "*/hello/*/bye");
    let mut s = String::from("$*$*$*/hello/$*/bye/$*");
    s.canonize();
    assert_eq!(s, "*/hello/*/bye/*");
    let mut s = String::from("$*$*$*/hello/$*$*/bye/$*$*");
    s.canonize();
    assert_eq!(s, "*/hello/*/bye/*");

    // **/* is replaced by */**
    let mut s = String::from("hello/**/*");
    s.canonize();
    assert_eq!(s, "hello/*/**");

    // &mut str remaining part is zeroed
    let mut s = String::from("$*$*$*/hello/$*$*/bye/$*$*");
    let mut s_mut = s.as_mut_str();
    s_mut.canonize();
    assert_eq!(s_mut, "*/hello/*/bye/*");
    assert_eq!(s, "*/hello/*/bye/*\0\0\0\0\0\0\0\0\0\0\0");
}
