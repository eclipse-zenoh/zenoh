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

#[cfg(feature = "auth_pubkey")]
mod auth_config {
    use std::path::PathBuf;

    use rsa::{
        pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding},
        BigUint, RsaPrivateKey, RsaPublicKey,
    };
    use zenoh_config::PubKeyConf;
    use zenoh_transport::unicast::establishment::ext::auth::{AuthPubKey, ZPublicKey};

    // Returns (pub_key, pri_key) using the same hardcoded key as client01 in unicast_authenticator.rs
    fn keypair() -> (RsaPublicKey, RsaPrivateKey) {
        let n = BigUint::from_bytes_le(&[
            0x41, 0x74, 0xc6, 0x40, 0x18, 0x63, 0xbd, 0x59, 0xe6, 0x0d, 0xe9, 0x23, 0x3e, 0x95,
            0xca, 0xb4, 0x5d, 0x17, 0x3d, 0x14, 0xdd, 0xbb, 0x16, 0x4a, 0x49, 0xeb, 0x43, 0x27,
            0x79, 0x3e, 0x75, 0x67, 0xd6, 0xf6, 0x7f, 0xe7, 0xbf, 0xb5, 0x1d, 0xf6, 0x27, 0x80,
            0xca, 0x26, 0x35, 0xa2, 0xc5, 0x4c, 0x96, 0x50, 0xaa, 0x9f, 0xf4, 0x47, 0xbe, 0x06,
            0x9c, 0xd1, 0xec, 0xfd, 0x1e, 0x81, 0xe9, 0xc4,
        ]);
        let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
        let pub_key = RsaPublicKey::new(n.clone(), e.clone()).unwrap();
        let d = BigUint::from_bytes_le(&[
            0x15, 0xe1, 0x93, 0xda, 0x75, 0xcb, 0x76, 0x40, 0xce, 0x70, 0x6f, 0x0f, 0x62, 0xe1,
            0x58, 0xa5, 0x53, 0x7b, 0x17, 0x63, 0x71, 0x70, 0x2d, 0x0d, 0xc5, 0xce, 0xcd, 0xb4,
            0x26, 0xe0, 0x22, 0x3d, 0xd4, 0x04, 0x88, 0x51, 0x24, 0x34, 0x01, 0x9c, 0x94, 0x01,
            0x47, 0x49, 0x86, 0xe3, 0x2f, 0x3b, 0x65, 0x5c, 0xcc, 0x0b, 0x8a, 0x00, 0x93, 0x26,
            0x79, 0xbb, 0x18, 0xab, 0x94, 0x4b, 0x52, 0x99,
        ]);
        let primes = vec![
            BigUint::from_bytes_le(&[
                0x87, 0x9c, 0xbd, 0x9c, 0xbf, 0xd5, 0xb7, 0xc2, 0x73, 0x16, 0x44, 0x3f, 0x67, 0x90,
                0xaa, 0xab, 0xfe, 0x20, 0xac, 0x7d, 0xe9, 0xc4, 0xb9, 0xfb, 0x12, 0xab, 0x09, 0x35,
                0xec, 0xf5, 0x9f, 0xe1,
            ]),
            BigUint::from_bytes_le(&[
                0xf7, 0xa2, 0xc1, 0x81, 0x63, 0xe1, 0x1c, 0x39, 0xe4, 0x7b, 0xbf, 0x56, 0xd5, 0x35,
                0xc3, 0xd1, 0x11, 0xf9, 0x1f, 0x42, 0x4e, 0x3e, 0xe7, 0xc9, 0xa2, 0x3c, 0x98, 0x08,
                0xaa, 0xf9, 0x6b, 0xdf,
            ]),
        ];
        let pri_key = RsaPrivateKey::from_components(n, e, d, primes).unwrap();
        (pub_key, pri_key)
    }

    fn keypair_pem() -> (String, String) {
        let (pub_key, pri_key) = keypair();
        let pub_pem = pub_key.to_pkcs1_pem(LineEnding::LF).unwrap().to_string();
        let pri_pem = pri_key.to_pkcs1_pem(LineEnding::LF).unwrap().to_string();
        (pub_pem, pri_pem)
    }

    // Returns a second distinct public key (client02 from unicast_authenticator.rs)
    fn alt_pub_key() -> RsaPublicKey {
        let n = BigUint::from_bytes_le(&[
            0xd1, 0x36, 0xcf, 0x94, 0xda, 0x04, 0x7e, 0x9f, 0x53, 0x39, 0xb8, 0x7b, 0x53, 0x3a,
            0xe6, 0xa4, 0x0e, 0x6c, 0xf0, 0x92, 0x5d, 0xd9, 0x1d, 0x84, 0xc3, 0x10, 0xab, 0x8f,
            0x7d, 0xe8, 0xf4, 0xff, 0x79, 0xae, 0x00, 0x25, 0xfc, 0xaf, 0x0c, 0x0f, 0x05, 0xc7,
            0xa3, 0xfd, 0x31, 0x9a, 0xd3, 0x79, 0x0f, 0x44, 0xa6, 0x1c, 0x19, 0x61, 0xed, 0xb0,
            0x27, 0x99, 0x53, 0x23, 0x50, 0xad, 0x67, 0xcf,
        ]);
        let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
        RsaPublicKey::new(n, e).unwrap()
    }

    fn write_tmp_file(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    async fn auth_config_pem_inline() {
        let (pub_pem, pri_pem) = keypair_pem();

        // empty config → None
        let config = PubKeyConf::default();
        assert!(AuthPubKey::from_config(&config).await.unwrap().is_none());

        // both keys inline → Some
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        assert!(AuthPubKey::from_config(&config).await.unwrap().is_some());

        // only public → Err
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // only private → Err
        let mut config = PubKeyConf::default();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());
    }

    #[tokio::test]
    async fn auth_config_pem_file() {
        let (pub_pem, pri_pem) = keypair_pem();
        let pub_path = write_tmp_file("zenoh-test-cfg-pub.pem", &pub_pem);
        let pri_path = write_tmp_file("zenoh-test-cfg-pri.pem", &pri_pem);

        // both files → Some
        let mut config = PubKeyConf::default();
        config
            .set_public_key_file(Some(pub_path.to_str().unwrap().to_owned()))
            .unwrap();
        config
            .set_private_key_file(Some(pri_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.unwrap().is_some());

        // only public file → Err
        let mut config = PubKeyConf::default();
        config
            .set_public_key_file(Some(pub_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // only private file → Err
        let mut config = PubKeyConf::default();
        config
            .set_private_key_file(Some(pri_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // nonexistent files → Err
        let mut config = PubKeyConf::default();
        config
            .set_public_key_file(Some("/nonexistent/pub.pem".to_owned()))
            .unwrap();
        config
            .set_private_key_file(Some("/nonexistent/pri.pem".to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        let _ = std::fs::remove_file(&pub_path);
        let _ = std::fs::remove_file(&pri_path);
    }

    // TDD: these tests document desired behavior not yet implemented.
    // public and private keys should be specifiable independently via different sources.
    #[tokio::test]
    async fn auth_config_cross_method() {
        let (pub_pem, pri_pem) = keypair_pem();
        let pub_path = write_tmp_file("zenoh-test-cfg-cross-pub.pem", &pub_pem);
        let pri_path = write_tmp_file("zenoh-test-cfg-cross-pri.pem", &pri_pem);

        // public from file, private from PEM → Some
        let mut config = PubKeyConf::default();
        config
            .set_public_key_file(Some(pub_path.to_str().unwrap().to_owned()))
            .unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        assert!(matches!(
            AuthPubKey::from_config(&config).await,
            Ok(Some(_))
        ));

        // public from PEM, private from file → Some
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config
            .set_private_key_file(Some(pri_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(matches!(
            AuthPubKey::from_config(&config).await,
            Ok(Some(_))
        ));

        let _ = std::fs::remove_file(&pub_path);
        let _ = std::fs::remove_file(&pri_path);
    }

    // TDD: each key must have exactly one source; providing two sources for the same key is an error.
    #[tokio::test]
    async fn auth_config_duplicate_source() {
        let (pub_pem, pri_pem) = keypair_pem();
        let pub_path = write_tmp_file("zenoh-test-cfg-dup-pub.pem", &pub_pem);
        let pri_path = write_tmp_file("zenoh-test-cfg-dup-pri.pem", &pri_pem);

        // public key has two sources → Err
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config
            .set_public_key_file(Some(pub_path.to_str().unwrap().to_owned()))
            .unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // private key has two sources → Err
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        config
            .set_private_key_file(Some(pri_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        let _ = std::fs::remove_file(&pub_path);
        let _ = std::fs::remove_file(&pri_path);
    }

    #[tokio::test]
    async fn auth_config_known_keys_file() {
        let (pub_pem, pri_pem) = keypair_pem();
        let key1_pub = ZPublicKey::from(keypair().0);
        let key2 = alt_pub_key();
        let key2_pem = key2.to_pkcs1_pem(LineEnding::LF).unwrap().to_string();
        let key2_pub = ZPublicKey::from(key2);

        // known_keys_file without a key pair → Err
        let keys_path = write_tmp_file("zenoh-test-cfg-known.pem", &pub_pem);
        let mut config = PubKeyConf::default();
        config
            .set_known_keys_file(Some(keys_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // malformed content → Err
        let bad_path = write_tmp_file(
            "zenoh-test-cfg-known-bad.pem",
            "not valid pem content\n-----END RSA PUBLIC KEY-----\n",
        );
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        config
            .set_known_keys_file(Some(bad_path.to_str().unwrap().to_owned()))
            .unwrap();
        assert!(AuthPubKey::from_config(&config).await.is_err());

        // single key in file → Some, lookup contains that key
        let single_path = write_tmp_file("zenoh-test-cfg-known-single.pem", &pub_pem);
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        config
            .set_known_keys_file(Some(single_path.to_str().unwrap().to_owned()))
            .unwrap();
        let auth = AuthPubKey::from_config(&config).await.unwrap().unwrap();
        assert!(auth.contains_known_key(&key1_pub));

        // two keys in file → Some, lookup contains both
        let two_keys_content = format!("{}{}", pub_pem, key2_pem);
        let two_path = write_tmp_file("zenoh-test-cfg-known-two.pem", &two_keys_content);
        let mut config = PubKeyConf::default();
        config.set_public_key_pem(Some(pub_pem.clone())).unwrap();
        config.set_private_key_pem(Some(pri_pem.clone())).unwrap();
        config
            .set_known_keys_file(Some(two_path.to_str().unwrap().to_owned()))
            .unwrap();
        let auth = AuthPubKey::from_config(&config).await.unwrap().unwrap();
        assert!(auth.contains_known_key(&key1_pub));
        assert!(auth.contains_known_key(&key2_pub));

        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_file(&bad_path);
        let _ = std::fs::remove_file(&single_path);
        let _ = std::fs::remove_file(&two_path);
    }
}
