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
mod common;

use std::{
    fs,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use itertools::Itertools;
use serde_json::json;
use tokio::runtime::Handle;
use zenoh::Session;
use zenoh_core::{zlock, ztimeout};

use crate::common::TestOpenBuilder;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const KEY_EXPR: &str = "test/demo";
const VALUE: &str = "zenoh";

#[allow(clippy::all)]
fn create_certs_dir() -> String {
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
    let certs_dir = std::env::temp_dir();
    for test_file in test_files {
        let file_path = certs_dir.join(test_file.name);
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(test_file.value).unwrap();
    }

    println!("testfiles created successfully.");
    certs_dir.to_string_lossy().to_string()
}

fn test_file(name: &str) -> String {
    static TESTFILES_PATH: OnceLock<String> = OnceLock::new();
    format!("{}/{name}", TESTFILES_PATH.get_or_init(create_certs_dir))
}

fn usrpwd_cfg(role: &str) -> serde_json::Value {
    json!({
         "user": format!("{role}name"),
         "password": format!("{role}passwd"),
         "dictionary_file": (role == "router").then(||test_file("credentials.txt"))
    })
}

enum TlsSide {
    Client,
    Server,
}

fn tls_cfg(side: TlsSide) -> serde_json::Value {
    let (role, side) = match side {
        TlsSide::Client => ("connect", "client"),
        TlsSide::Server => ("listen", "server"),
    };
    json!({
        "enable_mtls": true,
        "verify_name_on_connect": false,
        format!("{role}_private_key"): test_file(&format!("{side}sidekey.pem")),
        format!("{role}_certificate"): test_file(&format!("{side}side.pem")),
        "root_ca_certificate": test_file("ca.pem"),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_usrpwd() {
    zenoh_util::init_log_from_env_or("error");
    test_pub_sub_deny_then_allow_usrpswd().await;
    test_pub_sub_allow_then_deny_usrpswd().await;
    test_get_qbl_allow_then_deny_usrpswd().await;
    test_get_qbl_deny_then_allow_usrpswd().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_tls() {
    zenoh_util::init_log_from_env_or("error");
    test_pub_sub_deny_then_allow_tls(false).await;
    test_pub_sub_allow_then_deny_tls().await;
    test_get_qbl_allow_then_deny_tls().await;
    test_get_qbl_deny_then_allow_tls().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_quic() {
    zenoh_util::init_log_from_env_or("error");
    test_pub_sub_deny_then_allow_quic().await;
    test_pub_sub_allow_then_deny_quic().await;
    test_get_qbl_deny_then_allow_quic().await;
    test_get_qbl_allow_then_deny_quic().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_lowlatency() {
    // Test link AuthIds accessibility for lowlatency transport
    zenoh_util::init_log_from_env_or("error");
    test_pub_sub_deny_then_allow_tls(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_subject_combinations() {
    zenoh_util::init_log_from_env_or("error");
    test_deny_allow_combination().await;
    test_allow_deny_combination().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_authentication_link_protocols() {
    test_pub_sub_auth_link_protocol().await
}

fn open_router<const N: usize>(protocols: [&str; N], acl_cfg: &str) -> TestOpenBuilder {
    println!("Opening router session");
    let endpoints = protocols
        .iter()
        .map(|p| format!("{p}/127.0.0.1:0"))
        .collect_vec();
    let mut router = common::open_router()
        .with_json5("access_control", acl_cfg)
        .with("listen/endpoints", endpoints)
        .with("transport/link/protocols", protocols.as_slice());
    if protocols.contains(&"tcp") {
        router = router.with("transport/auth/usrpwd", usrpwd_cfg("router"));
    }
    if protocols.contains(&"quic") || protocols.contains(&"tls") {
        router = router.with("transport/link/tls", tls_cfg(TlsSide::Server));
    }
    router
}

fn open_router_usrpswd(acl_cfg: &str) -> TestOpenBuilder {
    open_router(["tcp"], acl_cfg)
}

fn open_router_tls(acl_cfg: &str, lowlatency: bool) -> TestOpenBuilder {
    open_router(["tls"], acl_cfg)
        .with("transport/unicast/lowlatency", lowlatency)
        .with("transport/unicast/qos/enabled", !lowlatency)
}

fn open_router_quic(acl_cfg: &str) -> TestOpenBuilder {
    open_router(["quic"], acl_cfg)
}

fn open_router_quic_usrpswd(acl_cfg: &str) -> TestOpenBuilder {
    open_router(["quic", "tcp"], acl_cfg)
}

async fn close_router_session(s: Session) {
    println!("Closing router session");
    ztimeout!(s.close()).unwrap();
}

async fn open_clients(
    router: &Session,
    protocol: &str,
    with: impl Fn(&str, TestOpenBuilder) -> TestOpenBuilder,
) -> (Session, Session) {
    println!("Opening client sessions");
    let open = |name: &str| {
        let mut client = common::open_client()
            .connect_to_protocol(router, protocol)
            .with("transport/link/protocols", [protocol]);
        client = match protocol {
            "tcp" => client.with("transport/auth/usrpwd", usrpwd_cfg(name)),
            "quic" | "tls" => client.with("transport/link/tls", tls_cfg(TlsSide::Client)),
            _ => unreachable!(),
        };
        with(name, client)
    };
    tokio::join!(open("client1"), open("client2"))
}

async fn open_client_sessions_tls(router: &Session, lowlatency: bool) -> (Session, Session) {
    open_clients(router, "tls", |_, client| {
        client
            .with("transport/unicast/lowlatency", lowlatency)
            .with("transport/unicast/qos/enabled", !lowlatency)
    })
    .await
}

async fn open_client_sessions_quic(router: &Session) -> (Session, Session) {
    open_clients(router, "quic", |_, client| client).await
}

async fn open_client_sessions_usrpswd(router: &Session) -> (Session, Session) {
    open_clients(router, "tcp", |_, client| client).await
}

async fn open_client_sessions_quic_usrpswd(router: &Session) -> (Session, Session) {
    open_clients(router, "quic", |name, client| {
        client.with("transport/auth/usrpwd", usrpwd_cfg(name))
    })
    .await
}

async fn close_client_sessions(s01: Session, s02: Session) {
    println!("Closing client sessions");
    ztimeout!(s01.close()).unwrap();
    ztimeout!(s02.close()).unwrap();
}

async fn test_pub_sub_deny_then_allow_tls(lowlatency: bool) {
    println!("test_pub_sub_deny_then_allow_tls");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_tls(acl_cfg, lowlatency));

    let (sub_session, pub_session) = ztimeout!(open_client_sessions_tls(&router, lowlatency));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_allow_then_deny_tls() {
    println!("test_pub_sub_allow_then_deny_tls");
    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_tls(acl_cfg, false));

    let (sub_session, pub_session) = ztimeout!(open_client_sessions_tls(&router, false));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_deny_then_allow_tls() {
    println!("test_get_qbl_deny_then_allow_tls");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_tls(acl_cfg, false));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_tls(&router, false));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_allow_then_deny_tls() {
    println!("test_get_qbl_allow_then_deny_tls");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_tls(acl_cfg, false));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_tls(&router, false));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_deny_then_allow_quic() {
    println!("test_pub_sub_deny_then_allow_quic");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic(acl_cfg));

    let (sub_session, pub_session) = ztimeout!(open_client_sessions_quic(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

#[allow(unused)]
async fn test_pub_sub_allow_then_deny_quic() {
    println!("test_pub_sub_allow_then_deny_quic");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_sessions_quic(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

#[allow(unused)]
async fn test_get_qbl_deny_then_allow_quic() {
    println!("test_get_qbl_deny_then_allow_quic");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic(acl_cfg));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_quic(&router));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

#[allow(unused)]
async fn test_get_qbl_allow_then_deny_quic() {
    println!("test_get_qbl_allow_then_deny_quic");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic(acl_cfg));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_quic(&router));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_deny_then_allow_usrpswd() {
    println!("test_pub_sub_deny_then_allow_usrpswd");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_usrpswd(acl_cfg));

    let (sub_session, pub_session) = ztimeout!(open_client_sessions_usrpswd(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_pub_sub_allow_then_deny_usrpswd() {
    println!("test_pub_sub_allow_then_deny_usrpswd");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_usrpswd(acl_cfg));

    let (sub_session, pub_session) = ztimeout!(open_client_sessions_usrpswd(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_deny_then_allow_usrpswd() {
    println!("test_get_qbl_deny_then_allow_usrpswd");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_usrpswd(acl_cfg));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_usrpswd(&router));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_get_qbl_allow_then_deny_usrpswd() {
    println!("test_get_qbl_allow_then_deny_usrpswd");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_usrpswd(acl_cfg));

    let (get_session, qbl_session) = ztimeout!(open_client_sessions_usrpswd(&router));
    {
        let mut received_value = String::new();

        let qbl = ztimeout!(qbl_session
            .declare_queryable(KEY_EXPR)
            .callback(move |sample| {
                tokio::task::block_in_place(move || {
                    Handle::current()
                        .block_on(async move { ztimeout!(sample.reply(KEY_EXPR, VALUE)).unwrap() });
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
    close_client_sessions(get_session, qbl_session).await;
    close_router_session(router).await;
}

async fn test_deny_allow_combination() {
    println!("test_deny_allow_combination");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic_usrpswd(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_sessions_usrpswd(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    let (sub_session, pub_session) = ztimeout!(open_client_sessions_quic_usrpswd(&router));
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
    close_router_session(router).await;
}

async fn test_allow_deny_combination() {
    println!("test_allow_deny_combination");

    let acl_cfg = r#"{
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
    }"#;
    let router = ztimeout!(open_router_quic_usrpswd(acl_cfg));
    let (sub_session, pub_session) = ztimeout!(open_client_sessions_usrpswd(&router));
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
    close_client_sessions(sub_session, pub_session).await;
    let (sub_session, pub_session) = ztimeout!(open_client_sessions_quic_usrpswd(&router));
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
    close_router_session(router).await;
}

async fn test_pub_sub_auth_link_protocol() {
    let key_expr = "acl_auth_test/pubsub/by_protocols";

    let acl_cfg = r#"{
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
    }"#;
    let listener_session = common::open_peer()
        .with("listen/endpoints", ["tcp/127.0.0.1:0", "udp/127.0.0.1:0"])
        .with("scouting/gossip/enabled", false)
        .with_json5("access_control", acl_cfg)
        .await;
    tokio::time::sleep(SLEEP).await;

    let session_denied = common::open_client()
        .with("scouting/gossip/enabled", false)
        .connect_to_protocol(&listener_session, "tcp")
        .await;

    let session_allowed = common::open_client()
        .with("scouting/gossip/enabled", false)
        .connect_to_protocol(&listener_session, "udp")
        .await;

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
