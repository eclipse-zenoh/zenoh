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
use aes::{
    cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit},
    Aes128,
};
use rand::Rng;
use zenoh_result::{bail, ZResult};

use super::PseudoRng;

pub struct BlockCipher {
    inner: Aes128,
}

impl BlockCipher {
    pub const BLOCK_SIZE: usize = 16;

    pub fn new(key: [u8; Self::BLOCK_SIZE]) -> BlockCipher {
        BlockCipher {
            inner: aes::Aes128::new(&key.into()),
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
            let block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.encrypt_block(block);
            start += Self::BLOCK_SIZE;
        }

        bytes
    }

    pub fn decrypt(&self, mut bytes: Vec<u8>) -> ZResult<Vec<u8>> {
        if bytes.len() % Self::BLOCK_SIZE != 0 {
            bail!("Invalid bytes length to decode: {}", bytes.len());
        }

        let mut start: usize = 0;
        while start < bytes.len() {
            let end = start + Self::BLOCK_SIZE;
            let block = GenericArray::from_mut_slice(&mut bytes[start..end]);
            self.inner.decrypt_block(block);
            start += Self::BLOCK_SIZE;
        }

        Ok(bytes)
    }
}

mod tests {
    #[test]
    fn cipher() {
        use rand::{RngCore, SeedableRng};

        use super::{BlockCipher, PseudoRng};

        fn encrypt_decrypt(cipher: &BlockCipher, prng: &mut PseudoRng) {
            println!("\n[1]");
            let t1 = "A".as_bytes().to_vec();
            println!("Clear: {t1:?}");
            let encrypted = cipher.encrypt(t1.clone(), prng);
            println!("Encrypted: {encrypted:?}");
            let decrypted = cipher.decrypt(encrypted).unwrap();
            println!("Decrypted: {decrypted:?}");
            assert_eq!(&t1[..], &decrypted[..t1.len()]);

            println!("\n[2]");
            let t2 = "Short string".as_bytes().to_vec();
            println!("Clear: {t2:?}");
            let encrypted = cipher.encrypt(t2.clone(), prng);
            println!("Encrypted: {encrypted:?}");
            let decrypted = cipher.decrypt(encrypted).unwrap();
            println!("Decrypted: {decrypted:?}");
            assert_eq!(&t2[..], &decrypted[..t2.len()]);

            println!("\n[3]");
            let t3 = "This is a medium string with some text".as_bytes().to_vec();
            println!("Clear: {t3:?}");
            let encrypted = cipher.encrypt(t3.clone(), prng);
            println!("Encrypted: {encrypted:?}");
            let decrypted = cipher.decrypt(encrypted).unwrap();
            println!("Decrypted: {decrypted:?}");
            assert_eq!(&t3[..], &decrypted[..t3.len()]);

            println!("\n[4]");
            let t4 = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.".as_bytes().to_vec();
            println!("Clear: {t4:?}");
            let encrypted = cipher.encrypt(t4.clone(), prng);
            println!("Encrypted: {encrypted:?}");
            let decrypted = cipher.decrypt(encrypted).unwrap();
            println!("Decrypted: {decrypted:?}");
            assert_eq!(&t4[..], &decrypted[..t4.len()]);
        }

        const RUN: usize = 16;

        let mut prng = PseudoRng::from_entropy();
        let mut key = [0_u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        for _ in 0..RUN {
            encrypt_decrypt(&cipher, &mut prng);
        }
    }
}
