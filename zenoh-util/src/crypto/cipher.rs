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
use super::PseudoRng;
use crate::core::{ZError, ZErrorKind, ZResult};
use crate::zerror;
use aes_soft::cipher::generic_array::GenericArray;
use aes_soft::cipher::{BlockCipher as AesBlockCipher, NewBlockCipher};
use aes_soft::Aes128;
use rand::Rng;

pub struct BlockCipher {
    inner: Aes128,
}

impl BlockCipher {
    pub const BLOCK_SIZE: usize = 16;

    pub fn new(key: [u8; Self::BLOCK_SIZE]) -> BlockCipher {
        BlockCipher {
            inner: aes_soft::Aes128::new(&key.into()),
        }
    }

    pub fn encrypt(&self, mut bytes: Vec<u8>, padding: &mut PseudoRng) -> Vec<u8> {
        let modulo = bytes.len() % Self::BLOCK_SIZE;
        if modulo != 0 {
            let missing = Self::BLOCK_SIZE - modulo;
            bytes.resize_with(bytes.len() + missing, || padding.gen::<u8>());
        }

        let mut start: usize = 0;
        while start < bytes.len() {
            let end = start + Self::BLOCK_SIZE;
            let mut block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.encrypt_block(&mut block);
            start += Self::BLOCK_SIZE;
        }

        bytes
    }

    pub fn decrypt(&self, mut bytes: Vec<u8>) -> ZResult<Vec<u8>> {
        if bytes.len() % Self::BLOCK_SIZE != 0 {
            let e = format!("Invalid bytes lenght to decode: {}", bytes.len());
            return zerror!(ZErrorKind::Other { descr: e });
        }

        let mut start: usize = 0;
        while start < bytes.len() {
            let end = start + Self::BLOCK_SIZE;
            let mut block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.decrypt_block(&mut block);
            start += Self::BLOCK_SIZE;
        }

        Ok(bytes)
    }
}
