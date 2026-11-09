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
//! Opens a TLS link while other clients fail their TLS handshakes on the same listener.
use std::time::{Duration, Instant};

use tokio::{io::AsyncWriteExt, net::TcpStream};
use zenoh_link_commons::{tls::config::*, LinkManagerUnicastTrait};
use zenoh_link_tls::LinkManagerUnicastTls;
use zenoh_protocol::core::EndPoint;

// A port scanner or a few dozen misconfigured clients reach this rate.
const FAILED_HANDSHAKES_PER_SECOND: u64 = 30;
const CHURN_BEFORE_OPEN: Duration = Duration::from_secs(2);
const OPEN_DEADLINE: Duration = Duration::from_secs(3);
const OPEN_LIMIT: Duration = Duration::from_secs(10);

// The target domain is localhost. Generated with https://github.com/jsha/minica
const KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
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

const CERT: &str = "-----BEGIN CERTIFICATE-----
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

const CA: &str = "-----BEGIN CERTIFICATE-----
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

fn endpoint(address: &str) -> EndPoint {
    let mut endpoint: EndPoint = format!("tls/{address}").parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CA),
                (TLS_LISTEN_PRIVATE_KEY_RAW, KEY),
                (TLS_LISTEN_CERTIFICATE_RAW, CERT),
            ]
            .iter()
            .copied(),
        )
        .unwrap();
    endpoint
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_opens_while_other_handshakes_fail() {
    let (new_link_tx, new_link_rx) = flume::unbounded();
    let manager = LinkManagerUnicastTls::new(new_link_tx);
    let locator = manager.new_listener(endpoint("localhost:0")).await.unwrap();
    let address = locator.address().as_str().to_owned();

    let churn = tokio::spawn({
        let address = address.clone();
        async move {
            let mut interval =
                tokio::time::interval(Duration::from_millis(1000 / FAILED_HANDSHAKES_PER_SECOND));
            loop {
                interval.tick().await;
                let address = address.clone();
                tokio::spawn(async move {
                    if let Ok(mut stream) = TcpStream::connect(address).await {
                        let _ = stream.write_all(b"not a TLS record").await;
                    }
                });
            }
        }
    });
    tokio::time::sleep(CHURN_BEFORE_OPEN).await;

    let start = Instant::now();
    let open = async {
        let client = manager.new_link(endpoint(&address)).await.unwrap();
        let server = new_link_rx.recv_async().await.unwrap();
        (client, server)
    };
    let res = tokio::time::timeout(OPEN_LIMIT, open).await;
    let elapsed = start.elapsed();
    churn.abort();
    assert!(res.is_ok(), "link did not open within {OPEN_LIMIT:?}");
    assert!(elapsed < OPEN_DEADLINE, "link took {elapsed:?} to open");
}
