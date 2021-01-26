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
use crate::core::{ZError, ZErrorKind, ZResult};
use crate::zerror;
use aes_soft::cipher::generic_array::GenericArray;
use aes_soft::cipher::{BlockCipher, NewBlockCipher};
use aes_soft::Aes128;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaChaRng;

const BLOCK_SIZE: usize = 16;

pub struct Cipher {
    inner: Aes128,
}

impl Cipher {
    pub fn new() -> Cipher {
        let mut key_bytes = [0u8; BLOCK_SIZE];
        let mut prng = ChaChaRng::from_entropy();
        prng.fill_bytes(&mut key_bytes);
        let key = GenericArray::from_slice(&key_bytes);
        let inner = aes_soft::Aes128::new(&key);

        Cipher { inner }
    }

    pub fn encrypt(&self, mut bytes: Vec<u8>) -> Vec<u8> {
        let modulo = bytes.len() % BLOCK_SIZE;
        let missing = if modulo == 0 { 0 } else { BLOCK_SIZE - modulo };
        bytes.resize(bytes.len() + missing, 0u8);

        let mut start: usize = 0;
        while start < bytes.len() {
            let end = start + BLOCK_SIZE;
            let mut block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.encrypt_block(&mut block);
            start += BLOCK_SIZE;
        }

        bytes
    }

    pub fn decrypt(&self, mut bytes: Vec<u8>) -> ZResult<Vec<u8>> {
        if bytes.len() % BLOCK_SIZE != 0 {
            let e = format!("Invalid bytes lenght to decode: {}", bytes.len());
            return zerror!(ZErrorKind::Other { descr: e });
        }

        let mut start: usize = 0;
        while start < bytes.len() {
            let end = start + BLOCK_SIZE;
            let mut block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.decrypt_block(&mut block);
            start += BLOCK_SIZE;
        }

        Ok(bytes)
    }
}

impl Default for Cipher {
    fn default() -> Cipher {
        Cipher::new()
    }
}
