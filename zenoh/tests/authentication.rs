//
// Copyright (c) 2024 ZettaScale Technology
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

#![cfg(feature = "internal_config")]

mod test {
    use std::{
        fs,
        path::PathBuf,
        str::FromStr,
        sync::{atomic::AtomicBool, Arc, Mutex},
        time::Duration,
    };

    use once_cell::sync::Lazy;
    use tokio::runtime::Handle;
    use zenoh::{
        config::{WhatAmI, ZenohId},
        Config, Session,
    };
    use zenoh_config::{EndPoint, ModeDependentValue};
    use zenoh_core::{zlock, ztimeout};

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    static TESTFILES_PATH: Lazy<PathBuf> = Lazy::new(std::env::temp_dir);
    static TESTFILES_CREATED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_usrpwd() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf())
            .await
            .unwrap();
        test_pub_sub_deny_then_allow_usrpswd(29447).await;
        test_pub_sub_allow_then_deny_usrpswd(29447).await;
        test_get_qbl_allow_then_deny_usrpswd(29447).await;
        test_get_qbl_deny_then_allow_usrpswd(29447).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_tls() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf())
            .await
            .unwrap();
        test_pub_sub_deny_then_allow_tls(29448, false).await;
        test_pub_sub_allow_then_deny_tls(29449).await;
        test_get_qbl_allow_then_deny_tls(29450).await;
        test_get_qbl_deny_then_allow_tls(29451).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_quic() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf())
            .await
            .unwrap();
        test_pub_sub_deny_then_allow_quic(29452).await;
        test_pub_sub_allow_then_deny_quic(29453).await;
        test_get_qbl_deny_then_allow_quic(29454).await;
        test_get_qbl_allow_then_deny_quic(29455).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_lowlatency() {
        // Test link AuthIds accessibility for lowlatency transport
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf())
            .await
            .unwrap();
        test_pub_sub_deny_then_allow_tls(29456, true).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_subject_combinations() {
        zenoh_util::init_log_from_env_or("error");
        create_new_files(TESTFILES_PATH.to_path_buf())
            .await
            .unwrap();
        test_deny_allow_combination(29457).await;
        test_allow_deny_combination(29458).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_link_protocols() {
        test_pub_sub_auth_link_protocol(1234).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication_zid() {
        zenoh_util::init_log_from_env_or("error");
        test_pub_sub_auth_zid(29459).await
    }

    #[allow(clippy::all)]
    async fn create_new_files(certs_dir: std::path::PathBuf) -> std::io::Result<()> {
        let created = TESTFILES_CREATED.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        if created {
            // only create files once per tests
            println!("Skipping testfile creation: files already created by another test instance");
            return Ok(());
        }
        use std::io::prelude::*;
        let ca_pem = b"-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIB42n1ZIkOakwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMwN1oYDzIxMjMw
MzA2MTYwMzA3WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAwNzhkYTcwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDIuCq24O4P4Aep5vAVlrIQ7P8+
uWWgcHIFYa02TmhBUB/hjo0JANCQvAtpVNuQ8NyKPlqnnq1cttePbSYVeA0rrnOs
DcfySAiyGBEY9zMjFfHJtH1wtrPcJEU8XIEY3xUlrAJE2CEuV9dVYgfEEydnvgLc
8Ug0WXSiARjqbnMW3l8jh6bYCp/UpL/gSM4mxdKrgpfyPoweGhlOWXc3RTS7cqM9
T25acURGOSI6/g8GF0sNE4VZmUvHggSTmsbLeXMJzxDWO+xVehRmbQx3IkG7u++b
QdRwGIJcDNn7zHlDMHtQ0Z1DBV94fZNBwCULhCBB5g20XTGw//S7Fj2FPwyhAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTWfAmQ/BUIQm/9
/llJJs2jUMWzGzAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzANBgkq
hkiG9w0BAQsFAAOCAQEAvtcZFAELKiTuOiAeYts6zeKxc+nnHCzayDeD/BDCbxGJ
e1n+xdHjLtWGd+/Anc+fvftSYBPTFQqCi84lPiUIln5z/rUxE+ke81hNPIfw2obc
yIg87xCabQpVyEh8s+MV+7YPQ1+fH4FuSi2Fck1FejxkVqN2uOZPvOYUmSTsaVr1
8SfRnwJNZ9UMRPM2bD4Jkvj0VcL42JM3QkOClOzYW4j/vll2cSs4kx7er27cIoo1
Ck0v2xSPAiVjg6w65rUQeW6uB5m0T2wyj+wm0At8vzhZPlgS1fKhcmT2dzOq3+oN
R+IdLiXcyIkg0m9N8I17p0ljCSkbrgGMD3bbePRTfg==
-----END CERTIFICATE-----";

        let client_side_pem = b"-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIeUtmIdFQznMwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMxOFoYDzIxMjMw
MzA2MTYwMzE4WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCx+oC6ESU3gefJ6oui9J3hB76c2/kDAKNI74cWIXfT
He9DUeKpEDRSbIWVKoGcUfdNQebglxp3jRB+tfx/XU0oZl2m8oewxipiNmdiREUZ
Lazh9DJoNtXkzTqzdQNfwRM+BjjVjx8IpNJV2L2IeTBxWtczFS7ggEHHQLWvYZKj
eCQgGdRwQt0V1pQ5Jt0KKkmFueTCLESvaHs9fHBtrtIhmBm1FpBZqTVUT1vvXqp7
eIy4yFoR+j9SgWZ5kI+7myl/Bo5mycKzFE+TYiNvOWwdMnT2Uz3CZsQUcExUBd6M
tOT75Kte3yMBJmE16f/YbPItA0Cq4af3yUIxDpKwT28tAgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAG/POnBob0S7iYwsbtI2
3LTTbRnmseIErtJuJmI9yYzgVIm6sUSKhlIUfAIm4rfRuzE94KFeWR2w9RabxOJD
wjYLLKvQ6rFY5g2AV/J0TwDjYuq0absdaDPZ8MKJ+/lpGYK3Te+CTOfq5FJRFt1q
GOkXAxnNpGg0obeRWRKFiAMHbcw6a8LIMfRjCooo3+uSQGsbVzGxSB4CYo720KcC
9vB1K9XALwzoqCewP4aiQsMY1GWpAmzXJftY3w+lka0e9dBYcdEdOqxSoZb5OBBZ
p5e60QweRuJsb60aUaCG8HoICevXYK2fFqCQdlb5sIqQqXyN2K6HuKAFywsjsGyJ
abY=
-----END CERTIFICATE-----";

        let client_side_key = b"-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAsfqAuhElN4HnyeqLovSd4Qe+nNv5AwCjSO+HFiF30x3vQ1Hi
qRA0UmyFlSqBnFH3TUHm4Jcad40QfrX8f11NKGZdpvKHsMYqYjZnYkRFGS2s4fQy
aDbV5M06s3UDX8ETPgY41Y8fCKTSVdi9iHkwcVrXMxUu4IBBx0C1r2GSo3gkIBnU
cELdFdaUOSbdCipJhbnkwixEr2h7PXxwba7SIZgZtRaQWak1VE9b716qe3iMuMha
Efo/UoFmeZCPu5spfwaOZsnCsxRPk2IjbzlsHTJ09lM9wmbEFHBMVAXejLTk++Sr
Xt8jASZhNen/2GzyLQNAquGn98lCMQ6SsE9vLQIDAQABAoIBAGQkKggHm6Q20L+4
2+bNsoOqguLplpvM4RMpyx11qWE9h6GeUmWD+5yg+SysJQ9aw0ZSHWEjRD4ePji9
lxvm2IIxzuIftp+NcM2gBN2ywhpfq9XbO/2NVR6PJ0dQQJzBG12bzKDFDdYkP0EU
WdiPL+WoEkvo0F57bAd77n6G7SZSgxYekBF+5S6rjbu5I1cEKW+r2vLehD4uFCVX
Q0Tu7TyIOE1KJ2anRb7ZXVUaguNj0/Er7EDT1+wN8KJKvQ1tYGIq/UUBtkP9nkOI
9XJd25k6m5AQPDddzd4W6/5+M7kjyVPi3CsQcpBPss6ueyecZOMaKqdWAHeEyaak
r67TofUCgYEA6GBa+YkRvp0Ept8cd5mh4gCRM8wUuhtzTQnhubCPivy/QqMWScdn
qD0OiARLAsqeoIfkAVgyqebVnxwTrKTvWe0JwpGylEVWQtpGz3oHgjST47yZxIiY
CSAaimi2CYnJZ+QB2oBkFVwNCuXdPEGX6LgnOGva19UKrm6ONsy6V9MCgYEAxBJu
fu4dGXZreARKEHa/7SQjI9ayAFuACFlON/EgSlICzQyG/pumv1FsMEiFrv6w7PRj
4AGqzyzGKXWVDRMrUNVeGPSKJSmlPGNqXfPaXRpVEeB7UQhAs5wyMrWDl8jEW7Ih
XcWhMLn1f/NOAKyrSDSEaEM+Nuu+xTifoAghvP8CgYEAlta9Fw+nihDIjT10cBo0
38w4dOP7bFcXQCGy+WMnujOYPzw34opiue1wOlB3FIfL8i5jjY/fyzPA5PhHuSCT
Ec9xL3B9+AsOFHU108XFi/pvKTwqoE1+SyYgtEmGKKjdKOfzYA9JaCgJe1J8inmV
jwXCx7gTJVjwBwxSmjXIm+sCgYBQF8NhQD1M0G3YCdCDZy7BXRippCL0OGxVfL2R
5oKtOVEBl9NxH/3+evE5y/Yn5Mw7Dx3ZPHUcygpslyZ6v9Da5T3Z7dKcmaVwxJ+H
n3wcugv0EIHvOPLNK8npovINR6rGVj6BAqD0uZHKYYYEioQxK5rGyGkaoDQ+dgHm
qku12wKBgQDem5FvNp5iW7mufkPZMqf3sEGtu612QeqejIPFM1z7VkUgetsgPBXD
tYsqC2FtWzY51VOEKNpnfH7zH5n+bjoI9nAEAW63TK9ZKkr2hRGsDhJdGzmLfQ7v
F6/CuIw9EsAq6qIB8O88FXQqald+BZOx6AzB8Oedsz/WtMmIEmr/+Q==
-----END RSA PRIVATE KEY-----";

        let server_side_pem = b"-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIeUtmIdFQznMwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMxOFoYDzIxMjMw
MzA2MTYwMzE4WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCx+oC6ESU3gefJ6oui9J3hB76c2/kDAKNI74cWIXfT
He9DUeKpEDRSbIWVKoGcUfdNQebglxp3jRB+tfx/XU0oZl2m8oewxipiNmdiREUZ
Lazh9DJoNtXkzTqzdQNfwRM+BjjVjx8IpNJV2L2IeTBxWtczFS7ggEHHQLWvYZKj
eCQgGdRwQt0V1pQ5Jt0KKkmFueTCLESvaHs9fHBtrtIhmBm1FpBZqTVUT1vvXqp7
eIy4yFoR+j9SgWZ5kI+7myl/Bo5mycKzFE+TYiNvOWwdMnT2Uz3CZsQUcExUBd6M
tOT75Kte3yMBJmE16f/YbPItA0Cq4af3yUIxDpKwT28tAgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAG/POnBob0S7iYwsbtI2
3LTTbRnmseIErtJuJmI9yYzgVIm6sUSKhlIUfAIm4rfRuzE94KFeWR2w9RabxOJD
wjYLLKvQ6rFY5g2AV/J0TwDjYuq0absdaDPZ8MKJ+/lpGYK3Te+CTOfq5FJRFt1q
GOkXAxnNpGg0obeRWRKFiAMHbcw6a8LIMfRjCooo3+uSQGsbVzGxSB4CYo720KcC
9vB1K9XALwzoqCewP4aiQsMY1GWpAmzXJftY3w+lka0e9dBYcdEdOqxSoZb5OBBZ
p5e60QweRuJsb60aUaCG8HoICevXYK2fFqCQdlb5sIqQqXyN2K6HuKAFywsjsGyJ
abY=
-----END CERTIFICATE-----";

        let server_side_key = b"-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAsfqAuhElN4HnyeqLovSd4Qe+nNv5AwCjSO+HFiF30x3vQ1Hi
qRA0UmyFlSqBnFH3TUHm4Jcad40QfrX8f11NKGZdpvKHsMYqYjZnYkRFGS2s4fQy
aDbV5M06s3UDX8ETPgY41Y8fCKTSVdi9iHkwcVrXMxUu4IBBx0C1r2GSo3gkIBnU
cELdFdaUOSbdCipJhbnkwixEr2h7PXxwba7SIZgZtRaQWak1VE9b716qe3iMuMha
Efo/UoFmeZCPu5spfwaOZsnCsxRPk2IjbzlsHTJ09lM9wmbEFHBMVAXejLTk++Sr
Xt8jASZhNen/2GzyLQNAquGn98lCMQ6SsE9vLQIDAQABAoIBAGQkKggHm6Q20L+4
2+bNsoOqguLplpvM4RMpyx11qWE9h6GeUmWD+5yg+SysJQ9aw0ZSHWEjRD4ePji9
lxvm2IIxzuIftp+NcM2gBN2ywhpfq9XbO/2NVR6PJ0dQQJzBG12bzKDFDdYkP0EU
WdiPL+WoEkvo0F57bAd77n6G7SZSgxYekBF+5S6rjbu5I1cEKW+r2vLehD4uFCVX
Q0Tu7TyIOE1KJ2anRb7ZXVUaguNj0/Er7EDT1+wN8KJKvQ1tYGIq/UUBtkP9nkOI
9XJd25k6m5AQPDddzd4W6/5+M7kjyVPi3CsQcpBPss6ueyecZOMaKqdWAHeEyaak
r67TofUCgYEA6GBa+YkRvp0Ept8cd5mh4gCRM8wUuhtzTQnhubCPivy/QqMWScdn
qD0OiARLAsqeoIfkAVgyqebVnxwTrKTvWe0JwpGylEVWQtpGz3oHgjST47yZxIiY
CSAaimi2CYnJZ+QB2oBkFVwNCuXdPEGX6LgnOGva19UKrm6ONsy6V9MCgYEAxBJu
fu4dGXZreARKEHa/7SQjI9ayAFuACFlON/EgSlICzQyG/pumv1FsMEiFrv6w7PRj
4AGqzyzGKXWVDRMrUNVeGPSKJSmlPGNqXfPaXRpVEeB7UQhAs5wyMrWDl8jEW7Ih
XcWhMLn1f/NOAKyrSDSEaEM+Nuu+xTifoAghvP8CgYEAlta9Fw+nihDIjT10cBo0
38w4dOP7bFcXQCGy+WMnujOYPzw34opiue1wOlB3FIfL8i5jjY/fyzPA5PhHuSCT
Ec9xL3B9+AsOFHU108XFi/pvKTwqoE1+SyYgtEmGKKjdKOfzYA9JaCgJe1J8inmV
jwXCx7gTJVjwBwxSmjXIm+sCgYBQF8NhQD1M0G3YCdCDZy7BXRippCL0OGxVfL2R
5oKtOVEBl9NxH/3+evE5y/Yn5Mw7Dx3ZPHUcygpslyZ6v9Da5T3Z7dKcmaVwxJ+H
n3wcugv0EIHvOPLNK8npovINR6rGVj6BAqD0uZHKYYYEioQxK5rGyGkaoDQ+dgHm
qku12wKBgQDem5FvNp5iW7mufkPZMqf3sEGtu612QeqejIPFM1z7VkUgetsgPBXD
tYsqC2FtWzY51VOEKNpnfH7zH5n+bjoI9nAEAW63TK9ZKkr2hRGsDhJdGzmLfQ7v
F6/CuIw9EsAq6qIB8O88FXQqald+BZOx6AzB8Oedsz/WtMmIEmr/+Q==
-----END RSA PRIVATE KEY-----";

        let credentials_txt = b"client1name:client1passwd
client2name:client2passwd";

        struct Testfile<'a> {
            name: &'a str,
            value: &'a [u8],
        }

        let test_files = vec![
            Testfile {
                name: "ca.pem",
                value: ca_pem,
            },
            Testfile {
                name: "clientsidekey.pem",
                value: client_side_key,
            },
            Testfile {
                name: "clientside.pem",
                value: client_side_pem,
            },
            Testfile {
                name: "serversidekey.pem",
                value: server_side_key,
            },
            Testfile {
                name: "serverside.pem",
                value: server_side_pem,
            },
            Testfile {
                name: "credentials.txt",
                value: credentials_txt,
            },
        ];
        for test_file in test_files {
            let file_path = certs_dir.join(test_file.name);
            let mut file = fs::File::create(&file_path)?;
            file.write_all(test_file.value)?;
        }

        println!("testfiles created successfully.");
        Ok(())
    }

    async fn get_basic_router_config_tls(port: u16, lowlatency: bool) -> Config {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![format!("tls/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        },
                    },
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_private_key(Some(format!("{cert_path}/serversidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_certificate(Some(format!("{cert_path}/serverside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        config.transport.unicast.set_lowlatency(lowlatency).unwrap();
        config
            .transport
            .unicast
            .qos
            .set_enabled(!lowlatency)
            .unwrap();
        config
    }
    async fn get_basic_router_config_quic(port: u16) -> Config {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![format!("quic/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "link": {
                    "protocols": [
                        "quic"
                    ],
                    "tls": {
                        "enable_mtls": true,
                        "verify_name_on_connect": false
                    },
                    },  
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_private_key(Some(format!("{cert_path}/serversidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_certificate(Some(format!("{cert_path}/serverside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        config
    }

    async fn get_basic_router_config_usrpswd(port: u16) -> Config {
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
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
            .set_dictionary_file(Some(format!(
                "{}/credentials.txt",
                TESTFILES_PATH.to_string_lossy()
            )))
            .unwrap();
        config
    }
    async fn close_router_session(s: Session) {
        println!("Closing router session");
        ztimeout!(s.close()).unwrap();
    }

    async fn get_basic_router_config_quic_usrpswd(port: u16) -> Config {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![
                format!("quic/127.0.0.1:{port}").parse().unwrap(),
                format!("tcp/127.0.0.1:{port}").parse().unwrap(),
            ])
            .unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "link": {
                        "protocols": [
                            "quic", "tcp"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        },
                    },
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
            .set_dictionary_file(Some(format!(
                "{}/credentials.txt",
                TESTFILES_PATH.to_string_lossy()
            )))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_private_key(Some(format!("{cert_path}/serversidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_listen_certificate(Some(format!("{cert_path}/serverside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        config
    }

    async fn get_client_sessions_tls(port: u16, lowlatency: bool) -> (Session, Session) {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        println!("Opening client sessions");
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tls/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        config.transport.unicast.set_lowlatency(lowlatency).unwrap();
        config
            .transport
            .unicast
            .qos
            .set_enabled(!lowlatency)
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();

        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tls/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        config.transport.unicast.set_lowlatency(lowlatency).unwrap();
        config
            .transport
            .unicast
            .qos
            .set_enabled(!lowlatency)
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_quic(port: u16) -> (Session, Session) {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        println!("Opening client sessions");
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "quic/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "quic/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_usrpswd(port: u16) -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tcp/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "auth": {
                        usrpwd: {
                            user: "client1name",
                            password: "client1passwd",
                        },
                    }
                }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tcp/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "auth": {
                        usrpwd: {
                            user: "client2name",
                            password: "client2passwd",
                        },
                    }
                }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_quic_usrpswd(port: u16) -> (Session, Session) {
        let cert_path = TESTFILES_PATH.to_string_lossy();
        println!("Opening client sessions");
        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "quic/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    },
                    "auth": {
                        usrpwd: {
                            user: "client1name",
                            password: "client1passwd",
                        },
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();

        let mut config = zenoh::Config::default();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "quic/127.0.0.1:{port}"
            )
            .parse::<EndPoint>()
            .unwrap()]))
            .unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "enable_mtls": true,
                            "verify_name_on_connect": false
                        }
                    },
                    "auth": {
                        usrpwd: {
                            user: "client2name",
                            password: "client2passwd",
                        },
                    }
                }"#,
            )
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_private_key(Some(format!("{cert_path}/clientsidekey.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_connect_certificate(Some(format!("{cert_path}/clientside.pem")))
            .unwrap();
        config
            .transport
            .link
            .tls
            .set_root_ca_certificate(Some(format!("{cert_path}/ca.pem")))
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn close_sessions(s01: Session, s02: Session) {
        println!("Closing client sessions");
        ztimeout!(s01.close()).unwrap();
        ztimeout!(s02.close()).unwrap();
    }

    async fn test_pub_sub_deny_then_allow_tls(port: u16, lowlatency: bool) {
        println!("test_pub_sub_deny_then_allow_tls");

        let mut config_router = get_basic_router_config_tls(port, lowlatency).await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["ingress","egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions_tls(port, lowlatency).await;
        {
            let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow_then_deny_tls(port: u16) {
        println!("test_pub_sub_allow_then_deny_tls");
        let mut config_router = get_basic_router_config_tls(port, false).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_tls(port, false).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny_then_allow_tls(port: u16) {
        println!("test_get_qbl_deny_then_allow_tls");

        let mut config_router = get_basic_router_config_tls(port, false).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply",
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls(port, false).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow_then_deny_tls(port: u16) {
        println!("test_get_qbl_allow_then_deny_tls");

        let mut config_router = get_basic_router_config_tls(port, false).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls(port, false).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_deny_then_allow_quic(port: u16) {
        println!("test_pub_sub_deny_then_allow_quic");

        let mut config_router = get_basic_router_config_quic(port).await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions_quic(port).await;
        {
            let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_pub_sub_allow_then_deny_quic(port: u16) {
        println!("test_pub_sub_allow_then_deny_quic");

        let mut config_router = get_basic_router_config_quic(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_quic(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_get_qbl_deny_then_allow_quic(port: u16) {
        println!("test_get_qbl_deny_then_allow_quic");

        let mut config_router = get_basic_router_config_quic(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["egress", "ingress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_quic(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    #[allow(unused)]
    async fn test_get_qbl_allow_then_deny_quic(port: u16) {
        println!("test_get_qbl_allow_then_deny_quic");

        let mut config_router = get_basic_router_config_quic(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_quic(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_deny_then_allow_usrpswd(port: u16) {
        println!("test_pub_sub_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd(port).await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["ingress", "egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions_usrpswd(port).await;
        {
            let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                })
                .await
                .unwrap();

            tokio::time::sleep(SLEEP).await;
            publisher.put(VALUE).await.unwrap();
            tokio::time::sleep(SLEEP).await;
            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_pub_sub_allow_then_deny_usrpswd(port: u16) {
        println!("test_pub_sub_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_usrpswd(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_deny_then_allow_usrpswd(port: u16) {
        println!("test_get_qbl_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["ingress", "egress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_eq!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_get_qbl_allow_then_deny_usrpswd(port: u16) {
        println!("test_get_qbl_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "query",
                                "declare_queryable",
                                "reply"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd(port).await;
        {
            let mut received_value = String::new();

            let qbl = ztimeout!(qbl_session
                .declare_queryable(KEY_EXPR)
                .callback(move |sample| {
                    tokio::task::block_in_place(move || {
                        Handle::current().block_on(async move {
                            ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap()
                        });
                    });
                }))
            .unwrap();

            tokio::time::sleep(SLEEP).await;
            let recv_reply = ztimeout!(get_session.get(KEY_EXPR)).unwrap();
            while let Ok(reply) = ztimeout!(recv_reply.recv_async()) {
                match reply.result() {
                    Ok(sample) => {
                        received_value = sample.payload().try_to_string().unwrap().into_owned();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .try_to_string()
                            .unwrap_or_else(|e| e.to_string().into())
                    ),
                }
            }
            tokio::time::sleep(SLEEP).await;
            assert_ne!(received_value, VALUE);
            ztimeout!(qbl.undeclare()).unwrap();
        }
        close_sessions(get_session, qbl_session).await;
        close_router_session(session).await;
    }

    async fn test_deny_allow_combination(port: u16) {
        println!("test_deny_allow_combination");

        let mut config_router = get_basic_router_config_quic_usrpswd(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "allow",
                            "flows": ["ingress", "egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");
        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_usrpswd(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        let (sub_session, pub_session) = get_client_sessions_quic_usrpswd(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_router_session(session).await;
    }

    async fn test_allow_deny_combination(port: u16) {
        println!("test_allow_deny_combination");

        let mut config_router = get_basic_router_config_quic_usrpswd(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["egress"],
                            "messages": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "cert_common_names": [
                                "localhost"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");
        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_usrpswd(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_eq!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_sessions(sub_session, pub_session).await;
        let (sub_session, pub_session) = get_client_sessions_quic_usrpswd(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().try_to_string().unwrap().into_owned();
                    }))
                .unwrap();

            tokio::time::sleep(SLEEP).await;

            ztimeout!(publisher.put(VALUE)).unwrap();
            tokio::time::sleep(SLEEP).await;

            assert_ne!(*zlock!(received_value), VALUE);
            ztimeout!(subscriber.undeclare()).unwrap();
        }
        close_router_session(session).await;
    }

    async fn test_pub_sub_auth_link_protocol(port: u16) {
        let key_expr = "acl_auth_test/pubsub/by_protocols";

        let mut config_listener = zenoh::Config::default();
        config_listener
            .listen
            .set_endpoints(ModeDependentValue::Unique(vec![
                format!("tcp/127.0.0.1:{port}").parse().unwrap(),
                format!("udp/127.0.0.1:{port}").parse().unwrap(),
            ]))
            .unwrap();
        config_listener
            .scouting
            .gossip
            .set_enabled(Some(false))
            .unwrap();
        config_listener
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();

        config_listener
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["ingress"],
                            "messages": [
                                "put",
                            ],
                            "key_exprs": [
                                "**"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "link_protocols": [ "tcp" ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        let listener_session = zenoh::open(config_listener).await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config_connect = zenoh::Config::default();
        config_connect.set_mode(Some(WhatAmI::Client)).unwrap();
        config_connect
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();
        config_connect
            .scouting
            .gossip
            .set_enabled(Some(false))
            .unwrap();
        config_connect
            .connect
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        let session_denied = zenoh::open(config_connect.clone()).await.unwrap();

        config_connect
            .connect
            .endpoints
            .set(vec![format!("udp/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        let session_allowed = zenoh::open(config_connect).await.unwrap();

        let sub = listener_session.declare_subscriber(key_expr).await.unwrap();

        session_denied.put(key_expr, "DENIED").await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(sub.try_recv().unwrap().is_none());

        session_allowed.put(key_expr, "ALLOWED").await.unwrap();
        tokio::time::sleep(SLEEP).await;
        let value = sub.recv_async().await;
        assert!(value.is_ok());
        let sample = value.unwrap();
        let payload = sample.payload().try_to_string().unwrap();
        assert!(payload.eq("ALLOWED"));

        sub.undeclare().await.unwrap();
        session_allowed.close().await.unwrap();
        session_denied.close().await.unwrap();
        listener_session.close().await.unwrap();
    }

    async fn test_pub_sub_auth_zid(port: u16) {
        let key_expr = "acl_auth_test/pubsub/by_zid";
        let test_zid = "abcdef";

        let mut config_listener = zenoh::Config::default();
        config_listener
            .listen
            .set_endpoints(ModeDependentValue::Unique(vec![format!(
                "tcp/127.0.0.1:{port}"
            )
            .parse()
            .unwrap()]))
            .unwrap();
        config_listener
            .scouting
            .gossip
            .set_enabled(Some(false))
            .unwrap();
        config_listener
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();

        config_listener
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "id": "r1",
                            "permission": "deny",
                            "flows": ["ingress"],
                            "messages": [
                                "put",
                            ],
                            "key_exprs": [
                                "**"
                            ],
                        },
                    ],
                    "subjects": [
                        {
                            "id": "s1",
                            "zids": [ "abcdef" ],
                        }
                    ],
                    "policies": [
                        {
                            "rules": ["r1"],
                            "subjects": ["s1"],
                        }
                    ]
                }"#,
            )
            .unwrap();

        let listener_session = zenoh::open(config_listener).await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config_connect = zenoh::Config::default();
        config_connect.set_mode(Some(WhatAmI::Client)).unwrap();
        config_connect
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();
        config_connect
            .scouting
            .gossip
            .set_enabled(Some(false))
            .unwrap();
        config_connect
            .connect
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
            .unwrap();
        let session_allowed = zenoh::open(config_connect.clone()).await.unwrap();

        config_connect
            .set_id(Some(ZenohId::from_str(test_zid).unwrap()))
            .unwrap();
        let session_denied = zenoh::open(config_connect).await.unwrap();

        let sub = listener_session.declare_subscriber(key_expr).await.unwrap();

        session_denied.put(key_expr, "DENIED").await.unwrap();
        tokio::time::sleep(SLEEP).await;
        assert!(sub.try_recv().unwrap().is_none());

        session_allowed.put(key_expr, "ALLOWED").await.unwrap();
        tokio::time::sleep(SLEEP).await;
        let value = sub.recv_async().await;
        assert!(value.is_ok());
        let sample = value.unwrap();
        let payload = sample.payload().try_to_string().unwrap();
        assert!(payload.eq("ALLOWED"));

        sub.undeclare().await.unwrap();
        session_allowed.close().await.unwrap();
        session_denied.close().await.unwrap();
        listener_session.close().await.unwrap();
    }
}
