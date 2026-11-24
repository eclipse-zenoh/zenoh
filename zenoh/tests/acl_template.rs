//
// Copyright (c) 2026 ZettaScale Technology
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

//! End-to-end tests for ACL key expression templates (`key_expr_templates`) combined
//! with certificate common name prefix subjects (`cert_common_name_prefixes`):
//! a single static rule `tenant/${cert_common_name}/**` must confine each TLS client
//! to its own key space, keyed by its authenticated certificate common name.

#![cfg(feature = "unstable")]

mod test {
    use std::{
        fs,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc, Mutex},
        time::Duration,
    };

    use once_cell::sync::Lazy;
    use zenoh::{config::WhatAmI, Session};
    use zenoh_config::{Config, EndPoint};
    use zenoh_core::{zlock, ztimeout};
    use zenoh_test::TestSessions;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    static TESTFILES_PATH: Lazy<PathBuf> = Lazy::new(std::env::temp_dir);
    static TESTFILES_CREATED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_template_tls() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf()).unwrap();
        test_tenant_isolation("tls").await;
        test_unmatched_common_name_falls_back_to_default_deny().await;
        test_deny_template_under_default_allow().await;
    }

    // the certificate common name is extracted from a different link auth branch for
    // QUIC than for TLS, so tenant isolation is exercised on both transports
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_template_quic() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf()).unwrap();
        test_tenant_isolation("quic").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_acl_template_usrpwd() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf()).unwrap();
        test_username_template_isolation().await;
    }

    #[allow(clippy::all)]
    fn create_new_files(certs_dir: std::path::PathBuf) -> std::io::Result<()> {
        let created = TESTFILES_CREATED.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        if created {
            // only create files once per tests
            println!("Skipping testfile creation: files already created by another test instance");
            return Ok(());
        }
        // CA and certificates generated for these tests only:
        //   ca:      CN=zenoh acl template test ca
        //   server:  CN=localhost (SAN: DNS:localhost, IP:127.0.0.1)
        //   tenants: CN=t1, CN=t2
        let ca_pem = b"-----BEGIN CERTIFICATE-----
MIIDLTCCAhWgAwIBAgIUc9NkY9fssA1ZruXMEiffnCiCf9EwDQYJKoZIhvcNAQEL
BQAwJTEjMCEGA1UEAwwaemVub2ggYWNsIHRlbXBsYXRlIHRlc3QgY2EwIBcNMjYw
NzAyMDQ0MTQ2WhgPMjEyNjA2MDgwNDQxNDZaMCUxIzAhBgNVBAMMGnplbm9oIGFj
bCB0ZW1wbGF0ZSB0ZXN0IGNhMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC
AQEAn2/WtzQyovN4MwFKKIvSwwyK/FzcLbD2QZVjBHRLEdrRdY9PkjSlYlrbb2wK
2ZH2W5zhYBQDUiCjULz2CLowcXsIYsqg5XwTEZHwbhT4gg2K/KzIHwhspJpyL0+x
sxEsUGYzw110PWGIq86FZjhwle9q3aysRP/MCO6Me4SKy2b6LGwVaEqkPMbUJ67j
f1Jwdh033RFmVftJiAi0zyQDJOXxvyFnDl35/vPirqysui2h1PF+c3UPnVAST2Sr
ATytFvCoS7Hko5fgqzPGAPGvYklR70HlIFyd03sQgPM0diemyOchrvxCP45ZZr7G
oAFw0IFWAyR+PdffHApFdKyM9wIDAQABo1MwUTAdBgNVHQ4EFgQUr7QLOLBEt3TT
74LKKtL7zVkkinAwHwYDVR0jBBgwFoAUr7QLOLBEt3TT74LKKtL7zVkkinAwDwYD
VR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAZ2eIWS3AdeOE0QLbQIoZ
W9naoChi/kgSn/3Iq8bl7TEAgyd0XyomQMGhjWkXQBN+L1ybHKrwN8l6osL9T+qk
emuNhgq0jjOyr9sGmvRq2IdbWDRee/a6TgYGg9jOTgrOqIzbyw8dd/jr++HXgpzx
ZWwHtDxsqRNG/DboKePjr5uxdInxHp9xBUx43HxhYDeDsW4EejJItGMFlrqqenXY
dmDBl0+ymT/mkR/74DzGfpf9P5baLVatNTqnYdNGP842c2y73Z8SuXofRm0LU7sT
aVreUeT3mhVZx3/2DeHyE2noqmqnEmmx9xHVehgWOzfxfRkByX+wzzHnQmnhEJmg
AQ==
-----END CERTIFICATE-----";

        let server_pem = b"-----BEGIN CERTIFICATE-----
MIIDYDCCAkigAwIBAgIUJrx816r+NMXPWF18WmaJchs+BqswDQYJKoZIhvcNAQEL
BQAwJTEjMCEGA1UEAwwaemVub2ggYWNsIHRlbXBsYXRlIHRlc3QgY2EwIBcNMjYw
NzAyMDQ0NzE4WhgPMjEyNjA2MDgwNDQ3MThaMBQxEjAQBgNVBAMMCWxvY2FsaG9z
dDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBANOT2bgF4BfIQv+FJVYL
gpKS4/5ZY/SxUCfym5mWjxbDKSf6XFxtCpck9ol2FJldtcarOqQeaT1BXHZzKgJY
E8Ur4hQz+9kOOAfu5vWZERk/Xk088VNHRBbegA4CPT1JM0nIGX+YhBAnFbCugd7/
jlKkI0pGDbIWdUO9Hi2uiKcCXV6/U58k46PTaKfjuQdWk1cjry5agI2tfiiT30Nx
ej09vX5dD9qAzXX2GQAKRBpzxr2DQ7jLF3lD7eZf3GFQn5Uxgz8gg4bZQciWdlkg
4BY6Lsa2NgPyko4dC7LtargGcBBm4uUzKO4ehuWkTpJ65ptJpDnLxHPPA9FLN6bQ
+r0CAwEAAaOBljCBkzAJBgNVHRMEAjAAMAsGA1UdDwQEAwIFoDAdBgNVHSUEFjAU
BggrBgEFBQcDAQYIKwYBBQUHAwIwGgYDVR0RBBMwEYIJbG9jYWxob3N0hwR/AAAB
MB0GA1UdDgQWBBRCgBGpxMUN7Ezr6yOrYB64S7IfBTAfBgNVHSMEGDAWgBSvtAs4
sES3dNPvgsoq0vvNWSSKcDANBgkqhkiG9w0BAQsFAAOCAQEALnQWpbirPhalaAb8
O+wZq0B2pxti1RGfSl36Kzfjy4iTc+7rRSJZP/jwHKmG0vbl7Q0X79C9cQzoVj7K
8phmbolXXLSzwL9X6X/lzgk/JkxfYIxKuweRdH1kBStt/1xn5U9sNIPUdRuPh0vT
fdL5UrqSRFEBPUPRuNTXnjiYfCfVJ9noEHLS55V6sJhu9boSJObFtgO8WJt6TPN9
yz8YiPjoGjcHm26J8S0ZPabvMqEIWeC6cicObvZnzwD+UUwDsCXqCtCDQXyiik57
TUrzpsk1HR9zCJ7urcb7UoMm22K8pv3G5C+VRRNBTBYdF8YtYnbvPxDHwfhfFnjx
H1ZmsQ==
-----END CERTIFICATE-----";

        let server_key = b"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDTk9m4BeAXyEL/
hSVWC4KSkuP+WWP0sVAn8puZlo8Wwykn+lxcbQqXJPaJdhSZXbXGqzqkHmk9QVx2
cyoCWBPFK+IUM/vZDjgH7ub1mREZP15NPPFTR0QW3oAOAj09STNJyBl/mIQQJxWw
roHe/45SpCNKRg2yFnVDvR4troinAl1ev1OfJOOj02in47kHVpNXI68uWoCNrX4o
k99DcXo9Pb1+XQ/agM119hkACkQac8a9g0O4yxd5Q+3mX9xhUJ+VMYM/IIOG2UHI
lnZZIOAWOi7GtjYD8pKOHQuy7Wq4BnAQZuLlMyjuHoblpE6SeuabSaQ5y8RzzwPR
Szem0Pq9AgMBAAECggEABL4qLF9wAso/HyAf2Xhh6n3lXEWcRR5lUdG04ednqqFn
kFWi2/P9zeUgOa31gZYsaTWxbNTZsGvWchzcHVRyyXOc01hyh7mIwx9uUDOMNJOS
aO6dd52OBr79rMR+v9RJW5jxYkYvalqdDaNRJKSUlitx/i+TYwUNao/716VsZeWI
/igpn+NG95+xKMajy7CttfTIgamgA4NFnKynTMWy0HMUpOXt4iPaqy0rOadIzsgE
15BdyYCI2qm6nZHSD1ed6vIZExX8KzqaQqXv0iF3j2i9jkSJq6KFt67g6xopDkIV
4hH/lIN/M4PabAS6aKIe+WJa+ZC4B9/qtGrtRdX8KQKBgQD5SRbRRpPqEVomJ07u
OVyH9LxW+qcXJDugY0Q3cIfzclJqVu/GDHnV0ReRx8ngeu0uLqsQQx9C9rh5gSGu
LHpXv7Eivr6VgsA+4OuL35IsSkvvDl8KgqzzJYKNhD4uEV9R+SeqRXfDKqt3ofSi
qTKwY3edIaRbDJzohDJmp7V/3wKBgQDZRsBwhkHySkGEQBpnf8oECFMWzEldwGMS
U+IIxWDxpK3L5ab3T9+jb03xVVJGw0c7lBgNfrcTQ/GT4jRe32RodQn1G+eJ+cdu
zXTQrF7REtuv62Z595OvhTt6KJJmNKVjnFwXnrFDUSXlHfZ/4L+CZUhD040DxF+L
WNEdsNJo4wKBgH9gME+Iv6W3bhfWuAcTuksh62aKNvylH+6JKl8lmeH0BVaey2+o
Ck0NxPxRWL7iMPULFY9+rKebx5EWQW5s/ap+oXU+f8WHhNHcPZ9AAsGsyPYCot+M
+/BVt0q2SsthRxJsvC7Nxi8sS2cakxTWXbcxa/oXKZL4c+h/O/2mLiCDAoGBANjI
SsD6c1m21N+Kxc12Jq0XUS9x69FqXm6u2ts9c5glYnIJVCl0vAFo0C91nX2U4MGE
5Oqx/x9trt6J1w7BfIDsJV801DNJz72xqFd7Y67eTeqbx8bxSZzaJZmgWVE4PbvB
CfFXGC2+DT0oRAUazHjhbNSfgha8G0gA+cPR5F1ZAoGBAJRNypjCEyzudFxdsQG2
Zj+8W7LjMrid5HYl7mvD/6bqmnTlKg1RHi0fHAJax7haLMobDFlKRHJSZ+V5UO4z
7IvbdfmDEBoDvYRWEfGvOFfRDohDJbVgQkkhJpH9N7sNNOKR89IRVL/YO+Owrizx
UcySqv8RyAaMBEe1E2S0apkO
-----END PRIVATE KEY-----";

        let t1_pem = b"-----BEGIN CERTIFICATE-----
MIIDMTCCAhmgAwIBAgIUJrx816r+NMXPWF18WmaJchs+BqkwDQYJKoZIhvcNAQEL
BQAwJTEjMCEGA1UEAwwaemVub2ggYWNsIHRlbXBsYXRlIHRlc3QgY2EwIBcNMjYw
NzAyMDQ0NzE4WhgPMjEyNjA2MDgwNDQ3MThaMA0xCzAJBgNVBAMMAnQxMIIBIjAN
BgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAxOKC7015O8mC/QEZHSPSqpbUAIDp
b8MgM2q5wHj4LXamPEsl07VN6yd1vneW8gQgkXBoBKDU1reRFIj903lDguBNNzRn
5m6K1FRAJ1mW+l1+/as8eZ9F615u8umZCxuRT46sT5lfac4x3L0GdPNyaAlmicJP
8UTv9yMqeILUrCroFnSUvKPcXTeHi1V1wpp5zMNh3MkRK9B4DcppWuPOtoX85Q6l
1IccxMTRHmRhZ/VRD9jrfmQafwiYanmDVf/Yph4wFIqWachtkXlcVMnt3Ox2mJN/
0QOFK+AvbwSckpMDefZHIZCECkhraAEzKinRvGMyRZAnLzVwpgoYNY366wIDAQAB
o28wbTAJBgNVHRMEAjAAMAsGA1UdDwQEAwIFoDATBgNVHSUEDDAKBggrBgEFBQcD
AjAdBgNVHQ4EFgQUCoWoYqaXSQhkaG/+zp1V2uFFNvUwHwYDVR0jBBgwFoAUr7QL
OLBEt3TT74LKKtL7zVkkinAwDQYJKoZIhvcNAQELBQADggEBAFUd9S42CUT62NC8
3j6lvA2sG+vhngMt9Uq/S29oIRTSa4jlOTcAMhpLhNdGA/T6/RypCIjhaptYB4+g
dVSxcN1roc1QzTGMyygJoRKmhro2ECfTSYRCU6yi58ylEQoEDyIT82nbAKszKLYX
iAUu/yF6yqPI1/7g3HALluPxfpHCxUvhJ1ujad2HvF+itBGznRXMxJ0W8XhPinFs
WX5iHcvabM3lmRWTt5qyXjXazEGORWPaYoQ4ZKennv+eo4+eS4IKO9521MWBt5oR
FQkdDK1ZnGHffJEJ9QJXb7aNmylZLjSyABaXNVuMz+evp4b1/+Q9w2BQGKZ6Mo0g
WuWoSvQ=
-----END CERTIFICATE-----";

        let t1_key = b"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDE4oLvTXk7yYL9
ARkdI9KqltQAgOlvwyAzarnAePgtdqY8SyXTtU3rJ3W+d5byBCCRcGgEoNTWt5EU
iP3TeUOC4E03NGfmborUVEAnWZb6XX79qzx5n0XrXm7y6ZkLG5FPjqxPmV9pzjHc
vQZ083JoCWaJwk/xRO/3Iyp4gtSsKugWdJS8o9xdN4eLVXXCmnnMw2HcyREr0HgN
ymla4862hfzlDqXUhxzExNEeZGFn9VEP2Ot+ZBp/CJhqeYNV/9imHjAUipZpyG2R
eVxUye3c7HaYk3/RA4Ur4C9vBJySkwN59kchkIQKSGtoATMqKdG8YzJFkCcvNXCm
Chg1jfrrAgMBAAECggEAF2AAhUfQm8a4x1qrhXhSKSW0xYXd8S0cnsN43+nyJnep
Pzb5e+amecOavkJ1iIXxQJiKCPlNUQbEnJ2ya7qVx4KOOFPU/W3ktqvKssFMuayD
CLvyofVFIwTNrbJf6gqGWIiLBaejGoKRYfCL795SwsOm9GETsQSVwY+KbySypOlS
YZwAQoIfp9dc5tM+utG8J9/TAfiY1c60EHAJTf2atgZDYtEqP97Jl4QwF0PKmJNN
AdmL53ZAXzKV1Ls4nZBD6XrAhCQ4ORIB3NqRL3JEZOs+P/KxvIcXUsevzM7HlADq
oQgfD49Ggg+tDV4UoFlfUs3aEWyCUGFJprR6p/wv1QKBgQDyvMi1FHd1dVEPkLFo
j9ksBsiLImZrL8raTiTqeyjb7I2X3WG6YErSH3jvFTCprqg6+NQrC13nRYBtro1E
YVT/uqpZ/oIWbHTkVrFXWd6dlq/E32/Soq6oUYW5BeHXA6TsyUNWe3TEI+/RtpfN
m5Cw/K+HPjG1mNKlLvKDcpEHvwKBgQDPpGC2/ZQHxzkerx80fGDnVNUtFDVF1r+4
s39AMirpGzGRtn7DLn6PyIvnzPxFwgX71c0CLMvv8AmDH8bY/Cw8/JVNdhCEEhzG
IQ4Fc4ga0epQjFiTaAvQzIBmkaEZqUz3YBUZI5VHjrZ21owoPf7PM9Z+Lj6s0xMZ
5vxAB+e31QKBgQC9frdI9dNUNOO1PQXiVPn7Lsh8JbzCzKqVxg93pfH9ziuzdLYI
Y4fFhaBJNMeqj5jxgLNRbyw9kbpy5aOO0FUk1rqKSu+PRdfzMeJ8CMKLT8mj7bJE
Q5AKAqpcCMWHr2afG3egGfzL6iocE2lqr5lDMeBtuhXgaI95OK9GArhJzQKBgQCL
nxh5c6GqaUf7Xf45qLjwVJbTrRb1UyWv6OLUI+e+v05hkLlEPWtU+6E3yRqJPaIQ
aP9lSwIG4P1EcoWfOlH04FL0t0L7y8IVZ/yppboLbsOEThrxY7EuQZTFY39UZgcf
ADivosGqUEhZOIMePDGu2kiMqEP0qinZ7PwJgkdJ0QKBgBqtbKtN64lUcwmKVHQr
XiNsLP2TXFZ9s11P/R1qGd1QNeACt1rlo77NAj2zwkMWDV8ii0XgCdh/0tTKbmNW
IVyOlyDfdYzGbLxm96TaGsLJb0DKFb4cg/FNpt7zRxu7fen4qH4Xe1R4lDBH1aaF
JroHOSe3du+eoiGGfcYlq64M
-----END PRIVATE KEY-----";

        let t2_pem = b"-----BEGIN CERTIFICATE-----
MIIDMTCCAhmgAwIBAgIUJrx816r+NMXPWF18WmaJchs+BqowDQYJKoZIhvcNAQEL
BQAwJTEjMCEGA1UEAwwaemVub2ggYWNsIHRlbXBsYXRlIHRlc3QgY2EwIBcNMjYw
NzAyMDQ0NzE4WhgPMjEyNjA2MDgwNDQ3MThaMA0xCzAJBgNVBAMMAnQyMIIBIjAN
BgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAq8QXuMJL2NNniDr4aeVVzwLAQtEw
J4kufCTD//EZtUhACNWH2zdClPM3TwQV5MAqlnE15oi8lmUZMwVYf+FcKifedGNs
NTdvQxl2ulNwoP/YkPV+EZgbg9tfpHBu+Nhy8+4K9/k16uWKQ1g432WRoXTDIWSk
t4mRkRGCWiBKsFDIUZCQp0GDUp7G/wAppLZFTKoayPFoZSs85gSgDjeADGw8v9b9
blFW4RWlCVBoeFYUIbEKEvoG8cGcXscCVONOWxHuE5WnjgFN4qXMPOxWvsOpIIBf
5xCXVQBMfztOnYyOBw/GNakWPFoH10F3g38+kjGif/m35Z0jkNKmc4C+SwIDAQAB
o28wbTAJBgNVHRMEAjAAMAsGA1UdDwQEAwIFoDATBgNVHSUEDDAKBggrBgEFBQcD
AjAdBgNVHQ4EFgQUguN5A1tZ1HPX+qEN3HKoO28gIicwHwYDVR0jBBgwFoAUr7QL
OLBEt3TT74LKKtL7zVkkinAwDQYJKoZIhvcNAQELBQADggEBABSsvCpOzUUxLWsd
+RkS8WKoqsE2yWqmKFwS+lXI38903WLDH20wkyOSs0Y69sYLJLQEsoAQ6zfZ8dM4
vk/VfdyEQi0xL0vzAjSDhdhh2Y+j2fE6C3jNhFQogSipieBWGJYkfj63+uluLRze
kfpSoJkai9g3fxVmlQ6BIs+pJSBXp9Ie/s3qlNQRFmTOVu5t95CJlUWjOjWrlBsV
S1n6K4FgO/Zt9bSjG+7Rr9iaCU1sisjctbebxydEaGgWmUOK+RxFsrFti36FOhih
3yXWYht+0QgQZwKflRer3RGDrozHYRNl7OHY0FPalA4YzWzN8sfPdXhZjCf5B7t2
1XmGaLI=
-----END CERTIFICATE-----";

        let t2_key = b"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCrxBe4wkvY02eI
Ovhp5VXPAsBC0TAniS58JMP/8Rm1SEAI1YfbN0KU8zdPBBXkwCqWcTXmiLyWZRkz
BVh/4VwqJ950Y2w1N29DGXa6U3Cg/9iQ9X4RmBuD21+kcG742HLz7gr3+TXq5YpD
WDjfZZGhdMMhZKS3iZGREYJaIEqwUMhRkJCnQYNSnsb/ACmktkVMqhrI8WhlKzzm
BKAON4AMbDy/1v1uUVbhFaUJUGh4VhQhsQoS+gbxwZxexwJU405bEe4TlaeOAU3i
pcw87Fa+w6kggF/nEJdVAEx/O06djI4HD8Y1qRY8WgfXQXeDfz6SMaJ/+bflnSOQ
0qZzgL5LAgMBAAECggEATe8NklN6BXm6DnovLyESo3gMkuSGNFIOaN5nnc+fifyr
rTZxS1oR2DJYZH4mjuFQEW1xdtWQt65MVjV1N6ShZDEtwmI//Q9XaLr7f0QPpMUg
1njEiCgbR+L3zM0E1Nykn5/gky2cNKWMa8zyFQ5pGrg3NwYKpIoDJa6rlcf0C3YE
ZHFNfEaJH7ntO/839z8mIAMc+HJdRoWT0IpHs/LnlZQyoxhbT6SV0ZS0nfV6F6YQ
l6QtTM61OGrsM8NSGEoWj2a8MmSLAgIUi6w5XnMSdcotKp2ul7OWP0pUKOddGEav
jx4IsEMno3GYQWElLjxNtvOVt6+pmtr/hiuAqeV3tQKBgQDpY6H2/7OcqFOAdYJE
jBXw/6nf96YcD7ZFkKLLCaHEwhGCyb6e806NGLAA1AJlrDMJ7oJeEFQdB9JnPtAf
TmLeI1mlP1a96+Yq5VbQW5IkyOcMX8QxjiG2cJIY4j8YHCGyjWRS9YfSgX6xFN+Q
SzKcx1CnTiF1BMmOYA/zedZyvwKBgQC8aB8Lhh/bzoRYtSDLgFChYL5/lVUL1Zo0
8cxcWDSbL84tweFV11U8I0wm7wxZCy5UKzHmoGC8/nhzXjjPGS6jk38AiCsMNAO9
mKfVzwljvpf1VK+nHmkH9ktC7m2rdfkrYRo7MpApOKGmpzvrMbEfruVvX8ErM+Ib
i3eddi3zdQKBgQC4IJqPO3yAg2wdVJfJbJuC3rEuuTqbuOmcSFemx5qQmGsoO/Hf
hSTbvDZe8ORTQl+h3kGL5GX34UvlmHCpwjXN+yWmcSoF/C5CeVzcVOIfk0B1SriG
QBPo0zbv2s7cPpV3QIV9zaeyM+e33Tfjpu/vMHA5DjLnFzfM04zCEcVWEQKBgHSS
4BKbTH8OiujwOXhwznLrjzMVzOdjpOR5b/77PKGAtMuvGKOqdrydAnNcmYFG38WI
bHnMZc7KjPClLfVGGYtwqbZEio4kaOQY3k/2qFKlDRTo7z4yHL6mb+7b49OhTSjA
DiDuqjA3MB4Tf4mI15VI/AEreDQpCBAO/VXaV5g5AoGBAIeZCZKy5lN9kftFmKme
dOJ6sTZprmD8Tpac7V0hP3XmethenOXd4kLKL3DRY682xOnXs4ddoPndm8DonKs3
CxZSBKuohwFEks3f71Fy8wjTP3WHGXwMnqvf0BHLAgihUBoA0xUenQn98wIRAr8N
mPZa/feTlKyltMOBwfXDKRHB
-----END PRIVATE KEY-----";

        let credentials = b"u1:p1
u2:p2";

        for (name, content) in [
            ("aclt_ca.pem", ca_pem.as_slice()),
            ("aclt_server.pem", server_pem.as_slice()),
            ("aclt_serverkey.pem", server_key.as_slice()),
            ("aclt_t1.pem", t1_pem.as_slice()),
            ("aclt_t1key.pem", t1_key.as_slice()),
            ("aclt_t2.pem", t2_pem.as_slice()),
            ("aclt_t2key.pem", t2_key.as_slice()),
            ("aclt_credentials.txt", credentials.as_slice()),
        ] {
            fs::write(certs_dir.join(name), content)?;
        }
        println!("testfiles created successfully.");
        Ok(())
    }

    fn get_locators(test_context: &TestSessions, protocol: &str) -> Vec<EndPoint> {
        test_context
            .locators()
            .iter()
            .filter(|endpoint| endpoint.protocol().as_str() == protocol)
            .cloned()
            .collect()
    }

    fn get_router_config(acl_json5: &str, protocol: &str) -> Config {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let mut config = zenoh_config::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![format!("{protocol}/127.0.0.1:0").parse().unwrap()])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                &format!(
                    r#"{{
                    "link": {{
                        "protocols": [
                            "{protocol}"
                        ],
                        "tls": {{
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }},
                    }},
                }}"#
                ),
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_private_key(Some(format!("{cert_path}/aclt_serverkey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_certificate(Some(format!("{cert_path}/aclt_server.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/aclt_ca.pem")))
            .unwrap();
        config.insert_json5("access_control", acl_json5).unwrap();
        config
    }

    async fn get_client_session(
        test_context: &mut TestSessions,
        cert_name: &str,
        protocol: &str,
    ) -> Session {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let locators = get_locators(test_context, protocol);
        let mut config = test_context.get_connector_config_with_endpoint(locators);
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .insert_json5(
                "transport",
                &format!(
                    r#"{{
                    "link": {{
                        "protocols": [
                            "{protocol}"
                        ],
                        "tls": {{
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }},
                    }},
                }}"#
                ),
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/aclt_{cert_name}key.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/aclt_{cert_name}.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/aclt_ca.pem")))
            .unwrap();
        test_context.open_connector_with_cfg(config).await
    }

    fn get_router_config_usrpwd(acl_json5: &str) -> Config {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let mut config = zenoh_config::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:0".parse().unwrap()])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "auth": {
                        usrpwd: {
                            user: "routername",
                            password: "routerpasswd",
                        },
                    },
                }"#,
            )
            .unwrap();
        config
            .transport
            .auth
            .usrpwd
            .set_dictionary_file(Some(format!("{cert_path}/aclt_credentials.txt")))
            .unwrap();
        config.insert_json5("access_control", acl_json5).unwrap();
        config
    }

    async fn get_client_session_usrpwd(
        test_context: &mut TestSessions,
        user: &str,
        password: &str,
    ) -> Session {
        let locators = get_locators(test_context, "tcp");
        let mut config = test_context.get_connector_config_with_endpoint(locators);
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .insert_json5(
                "transport",
                &format!(
                    r#"{{
                    "auth": {{
                        usrpwd: {{
                            user: "{user}",
                            password: "{password}",
                        }},
                    }},
                }}"#
                ),
            )
            .unwrap();
        test_context.open_connector_with_cfg(config).await
    }

    const TENANT_ACL: &str = r#"{
        "enabled": true,
        "default_permission": "deny",
        "rules": [
            {
                "id": "tenant-own-namespace",
                "permission": "allow",
                "flows": ["ingress", "egress"],
                "messages": ["put", "declare_subscriber"],
                "key_expr_templates": ["tenant/${cert_common_name}/**"],
            },
        ],
        "subjects": [
            {
                "id": "tenants",
                "cert_common_name_prefixes": ["t"],
            },
        ],
        "policies": [
            {
                "rules": ["tenant-own-namespace"],
                "subjects": ["tenants"],
            },
        ],
    }"#;

    /// Each tenant may only publish/subscribe under `tenant/<its own common name>/**`.
    async fn test_tenant_isolation(protocol: &str) {
        println!("test_tenant_isolation ({protocol})");
        let mut test_context = TestSessions::new();
        let _router = test_context
            .open_listener_with_cfg(get_router_config(TENANT_ACL, protocol))
            .await;

        // pub and sub sessions are distinct so that samples always go through the router
        // (same-session pub/sub is delivered locally, bypassing ACL)
        let t1_pub_session = get_client_session(&mut test_context, "t1", protocol).await;
        let t1_sub_session = get_client_session(&mut test_context, "t1", protocol).await;
        let t2_pub_session = get_client_session(&mut test_context, "t2", protocol).await;
        let t2_sub_session = get_client_session(&mut test_context, "t2", protocol).await;
        {
            let t1_received = Arc::new(Mutex::new(String::new()));
            let cb_value = t1_received.clone();
            let t1_sub = t1_sub_session
                .declare_subscriber("tenant/t1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            let t2_received_own = Arc::new(Mutex::new(String::new()));
            let cb_value = t2_received_own.clone();
            let t2_sub_own = t2_sub_session
                .declare_subscriber("tenant/t2/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            // t2 subscribing to t1's key space must be blocked by the router
            let t2_received_foreign = Arc::new(Mutex::new(String::new()));
            let cb_value = t2_received_foreign.clone();
            let t2_sub_foreign = t2_sub_session
                .declare_subscriber("tenant/t1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            // each tenant reaches its own key space
            t1_pub_session
                .put("tenant/t1/demo", "t1-value")
                .await
                .unwrap();
            t2_pub_session
                .put("tenant/t2/demo", "t2-value")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(t1_received), "t1-value");
            assert_eq!(*zlock!(t2_received_own), "t2-value");
            // ...but t2 never sees t1's data
            assert_eq!(*zlock!(t2_received_foreign), "");

            // t2 publishing into t1's key space is dropped at ingress
            t2_pub_session
                .put("tenant/t1/demo", "cross-tenant")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(t1_received), "t1-value");

            ztimeout!(t1_sub.undeclare()).unwrap();
            ztimeout!(t2_sub_own.undeclare()).unwrap();
            ztimeout!(t2_sub_foreign.undeclare()).unwrap();
        }
        test_context.close().await;
    }

    /// A client whose common name does not match any subject gets the default permission.
    async fn test_unmatched_common_name_falls_back_to_default_deny() {
        println!("test_unmatched_common_name_falls_back_to_default_deny");
        let mut test_context = TestSessions::new();
        // same policy, but subjects only match common names starting with "t1"
        let acl = TENANT_ACL.replace(
            r#""cert_common_name_prefixes": ["t"]"#,
            r#""cert_common_name_prefixes": ["t1"]"#,
        );
        let _router = test_context
            .open_listener_with_cfg(get_router_config(&acl, "tls"))
            .await;

        // distinct pub/sub sessions: samples must go through the router (see above)
        let t1_pub_session = get_client_session(&mut test_context, "t1", "tls").await;
        let t1_sub_session = get_client_session(&mut test_context, "t1", "tls").await;
        let t2_pub_session = get_client_session(&mut test_context, "t2", "tls").await;
        let t2_sub_session = get_client_session(&mut test_context, "t2", "tls").await;
        {
            let t1_received = Arc::new(Mutex::new(String::new()));
            let cb_value = t1_received.clone();
            let t1_sub = t1_sub_session
                .declare_subscriber("tenant/t1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            let t2_received = Arc::new(Mutex::new(String::new()));
            let cb_value = t2_received.clone();
            let t2_sub = t2_sub_session
                .declare_subscriber("tenant/t2/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            t1_pub_session
                .put("tenant/t1/demo", "t1-value")
                .await
                .unwrap();
            // t2 no longer matches any subject: default deny applies to everything
            t2_pub_session
                .put("tenant/t2/demo", "t2-value")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(t1_received), "t1-value");
            assert_eq!(*zlock!(t2_received), "");

            ztimeout!(t1_sub.undeclare()).unwrap();
            ztimeout!(t2_sub.undeclare()).unwrap();
        }
        test_context.close().await;
    }

    /// `${username}` templates confine each user/password-authenticated client to its
    /// own key space. The subject is a wildcard (matches every connection): clients
    /// without a username simply never match the template, falling back to default deny.
    async fn test_username_template_isolation() {
        println!("test_username_template_isolation");
        let mut test_context = TestSessions::new();
        let acl = r#"{
            "enabled": true,
            "default_permission": "deny",
            "rules": [
                {
                    "id": "user-own-namespace",
                    "permission": "allow",
                    "flows": ["ingress", "egress"],
                    "messages": ["put", "declare_subscriber"],
                    "key_expr_templates": ["tenant/${username}/**"],
                },
            ],
            "subjects": [
                {
                    "id": "users",
                },
            ],
            "policies": [
                {
                    "rules": ["user-own-namespace"],
                    "subjects": ["users"],
                },
            ],
        }"#;
        let _router = test_context
            .open_listener_with_cfg(get_router_config_usrpwd(acl))
            .await;

        // distinct pub/sub sessions: samples must go through the router (see above)
        let u1_pub_session = get_client_session_usrpwd(&mut test_context, "u1", "p1").await;
        let u1_sub_session = get_client_session_usrpwd(&mut test_context, "u1", "p1").await;
        let u2_pub_session = get_client_session_usrpwd(&mut test_context, "u2", "p2").await;
        let u2_sub_session = get_client_session_usrpwd(&mut test_context, "u2", "p2").await;
        {
            let u1_received = Arc::new(Mutex::new(String::new()));
            let cb_value = u1_received.clone();
            let u1_sub = u1_sub_session
                .declare_subscriber("tenant/u1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            let u2_received_own = Arc::new(Mutex::new(String::new()));
            let cb_value = u2_received_own.clone();
            let u2_sub_own = u2_sub_session
                .declare_subscriber("tenant/u2/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            // u2 subscribing to u1's key space must be blocked by the router
            let u2_received_foreign = Arc::new(Mutex::new(String::new()));
            let cb_value = u2_received_foreign.clone();
            let u2_sub_foreign = u2_sub_session
                .declare_subscriber("tenant/u1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            u1_pub_session
                .put("tenant/u1/demo", "u1-value")
                .await
                .unwrap();
            u2_pub_session
                .put("tenant/u2/demo", "u2-value")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(u1_received), "u1-value");
            assert_eq!(*zlock!(u2_received_own), "u2-value");
            assert_eq!(*zlock!(u2_received_foreign), "");

            // u2 publishing into u1's key space is dropped at ingress
            u2_pub_session
                .put("tenant/u1/demo", "cross-user")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(u1_received), "u1-value");

            ztimeout!(u1_sub.undeclare()).unwrap();
            ztimeout!(u2_sub_own.undeclare()).unwrap();
            ztimeout!(u2_sub_foreign.undeclare()).unwrap();
        }
        test_context.close().await;
    }

    /// A deny template rule must not be overridden by its own subject falling back to
    /// the default permission (regression test: template rules are evaluated as part
    /// of their subject, not as a separate one).
    async fn test_deny_template_under_default_allow() {
        println!("test_deny_template_under_default_allow");
        let mut test_context = TestSessions::new();
        let acl = r#"{
            "enabled": true,
            "default_permission": "allow",
            "rules": [
                {
                    "id": "deny-private",
                    "permission": "deny",
                    "flows": ["ingress"],
                    "messages": ["put"],
                    "key_expr_templates": ["tenant/${cert_common_name}/private/**"],
                },
            ],
            "subjects": [
                {
                    "id": "tenants",
                    "cert_common_name_prefixes": ["t"],
                },
            ],
            "policies": [
                {
                    "rules": ["deny-private"],
                    "subjects": ["tenants"],
                },
            ],
        }"#;
        let _router = test_context
            .open_listener_with_cfg(get_router_config(acl, "tls"))
            .await;

        let t1_pub_session = get_client_session(&mut test_context, "t1", "tls").await;
        let t1_sub_session = get_client_session(&mut test_context, "t1", "tls").await;
        {
            let received = Arc::new(Mutex::new(String::new()));
            let cb_value = received.clone();
            let t1_sub = t1_sub_session
                .declare_subscriber("tenant/t1/**")
                .callback(move |sample| {
                    let mut value = zlock!(cb_value);
                    *value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            // puts outside the denied subspace pass (default allow)
            t1_pub_session
                .put("tenant/t1/public/demo", "public-value")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received), "public-value");

            // puts into the denied subspace are dropped at ingress
            t1_pub_session
                .put("tenant/t1/private/secret", "private-value")
                .await
                .unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received), "public-value");

            ztimeout!(t1_sub.undeclare()).unwrap();
        }
        test_context.close().await;
    }
}
