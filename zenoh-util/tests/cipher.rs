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
use rand::{RngCore, SeedableRng};
use zenoh_util::crypto::{BlockCipher, PseudoRng, BLOCK_SIZE};

const RUN: usize = 16;

fn encrypt_decrypt(cipher: &BlockCipher, prng: &mut PseudoRng) {
    println!("\n[1]");
    let t1 = "A".as_bytes().to_vec();
    println!("Clear: {:?}", t1);
    let encrypted = cipher.encrypt(t1.clone(), prng);
    println!("Encrypted: {:?}", encrypted);
    let decrypted = cipher.decrypt(encrypted).unwrap();
    println!("Decrypted: {:?}", decrypted);
    assert_eq!(&t1[..], &decrypted[..t1.len()]);

    println!("\n[2]");
    let t2 = "Short string".as_bytes().to_vec();
    println!("Clear: {:?}", t2);
    let encrypted = cipher.encrypt(t2.clone(), prng);
    println!("Encrypted: {:?}", encrypted);
    let decrypted = cipher.decrypt(encrypted).unwrap();
    println!("Decrypted: {:?}", decrypted);
    assert_eq!(&t2[..], &decrypted[..t2.len()]);

    println!("\n[3]");
    let t3 = "This is a medium string with some text".as_bytes().to_vec();
    println!("Clear: {:?}", t3);
    let encrypted = cipher.encrypt(t3.clone(), prng);
    println!("Encrypted: {:?}", encrypted);
    let decrypted = cipher.decrypt(encrypted).unwrap();
    println!("Decrypted: {:?}", decrypted);
    assert_eq!(&t3[..], &decrypted[..t3.len()]);

    println!("\n[4]");
    let t4 = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.".as_bytes().to_vec();
    println!("Clear: {:?}", t4);
    let encrypted = cipher.encrypt(t4.clone(), prng);
    println!("Encrypted: {:?}", encrypted);
    let decrypted = cipher.decrypt(encrypted).unwrap();
    println!("Decrypted: {:?}", decrypted);
    assert_eq!(&t4[..], &decrypted[..t4.len()]);
}
#[test]
fn cipher() {
    let mut prng = PseudoRng::from_entropy();
    let mut key = [0u8; BLOCK_SIZE];
    prng.fill_bytes(&mut key);
    let cipher = BlockCipher::new(key);

    for _ in 0..RUN {
        encrypt_decrypt(&cipher, &mut prng);
    }
}
