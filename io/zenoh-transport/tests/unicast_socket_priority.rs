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
#[cfg(all(feature = "transport_multilink", target_os = "linux"))]
mod tests {
    use std::{
        collections::HashMap,
        convert::TryFrom,
        os::fd::{AsRawFd, BorrowedFd},
        sync::Arc,
        time::Duration,
    };

    use zenoh_config::SocketPriorityConf;
    use zenoh_core::ztimeout;
    use zenoh_link::EndPoint;
    use zenoh_protocol::core::{Metadata, WhatAmI, ZenohIdProto};
    use zenoh_result::ZResult;
    use zenoh_transport::{
        multicast::TransportMulticast, unicast::TransportUnicast, DummyTransportPeerEventHandler,
        TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
        TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_millis(100);

    struct SH;

    impl TransportEventHandler for SH {
        fn new_unicast(
            &self,
            _peer: TransportPeer,
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            Ok(Arc::new(DummyTransportPeerEventHandler))
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    fn manager(id: u8, whatami: WhatAmI, socket_priority: &SocketPriorityConf) -> TransportManager {
        TransportManager::builder()
            .whatami(whatami)
            .zid(ZenohIdProto::try_from([id]).unwrap())
            .unicast(TransportManager::config_unicast().max_links(2))
            .socket_priority(socket_priority.clone())
            .build_test(Arc::new(SH))
            .unwrap()
    }

    /// The connected TCP sockets of this process with `port` at one end, as
    /// (local port, peer port) -> SO_PRIORITY.
    fn socket_priorities(port: u16) -> HashMap<(u16, u16), u32> {
        let mut out = HashMap::new();
        for entry in std::fs::read_dir("/proc/self/fd").unwrap() {
            let Ok(fd) = entry.unwrap().file_name().to_string_lossy().parse::<i32>() else {
                continue;
            };
            // SAFETY: the fd is only used while the transports that own it are open.
            let fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let socket = socket2::SockRef::from(&fd);
            let (Ok(local), Ok(peer)) = (socket.local_addr(), socket.peer_addr()) else {
                continue;
            };
            let (Some(local), Some(peer)) = (local.as_socket(), peer.as_socket()) else {
                continue;
            };
            if local.port() != port && peer.port() != port {
                continue;
            }
            let mut value: libc::c_int = 0;
            let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            // SAFETY: `value` and `len` outlive the call and match SO_PRIORITY.
            let ret = unsafe {
                libc::getsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PRIORITY,
                    &mut value as *mut libc::c_int as *mut libc::c_void,
                    &mut len,
                )
            };
            assert_eq!(ret, 0);
            out.insert((local.port(), peer.port()), value as u32);
        }
        out
    }

    /// Opens `endpoints` from a client to a router that both use `socket_priority`, and returns the
    /// socket priority of each connection as (client side, router side).
    async fn link_priorities(
        listen: &EndPoint,
        endpoints: &[EndPoint],
        socket_priority: &SocketPriorityConf,
    ) -> Vec<(u32, u32)> {
        let router = manager(1, WhatAmI::Router, socket_priority);
        let client = manager(2, WhatAmI::Client, socket_priority);
        ztimeout!(router.add_listener(listen.clone())).unwrap();
        for endpoint in endpoints {
            ztimeout!(client.open_transport_unicast(endpoint.clone())).unwrap();
        }
        ztimeout!(async {
            loop {
                let transports = router.get_transports_unicast().await;
                if transports
                    .first()
                    .is_some_and(|t| t.get_links().unwrap().len() == endpoints.len())
                {
                    break;
                }
                tokio::time::sleep(SLEEP).await;
            }
        });

        let port = listen
            .address()
            .as_str()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let sockets = socket_priorities(port);
        let mut out: Vec<(u32, u32)> = sockets
            .iter()
            .filter(|((_, peer), _)| *peer == port)
            .map(|((local, _), client_side)| (*client_side, sockets[&(port, *local)]))
            .collect();
        out.sort();

        ztimeout!(client.close());
        ztimeout!(router.close());
        out
    }

    fn enabled() -> SocketPriorityConf {
        let mut conf = SocketPriorityConf::default();
        conf.set_enabled(true).unwrap();
        conf.map.set_control(5).unwrap();
        conf.map.set_data_low(1).unwrap();
        conf
    }

    fn with_prio(endpoint: &EndPoint, prio: &str) -> EndPoint {
        let mut endpoint = endpoint.clone();
        endpoint
            .metadata_mut()
            .insert(Metadata::PRIORITIES, prio)
            .unwrap();
        endpoint
    }

    /// `new_endpoint` gives an endpoint on a free port, so that each case sees only its own sockets.
    async fn check(new_endpoint: impl Fn() -> EndPoint) {
        let listen = new_endpoint();
        let split = [with_prio(&listen, "0-5"), with_prio(&listen, "6-7")];
        assert_eq!(
            link_priorities(&listen, &split, &enabled()).await,
            [(1, 1), (5, 5)]
        );

        let listen = new_endpoint();
        let split = [with_prio(&listen, "0-5"), with_prio(&listen, "6-7")];
        assert_eq!(
            link_priorities(&listen, &split, &SocketPriorityConf::default()).await,
            [(0, 0), (0, 0)]
        );

        let listen = new_endpoint();
        assert_eq!(
            link_priorities(&listen, std::slice::from_ref(&listen), &enabled()).await,
            [(0, 0)]
        );
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn socket_priority_tcp() {
        zenoh_util::init_log_from_env_or("error");
        check(|| {
            format!("tcp/127.0.0.1:{}", zenoh_test::get_free_tcp_port())
                .parse()
                .unwrap()
        })
        .await;
    }

    #[cfg(feature = "transport_tls")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn socket_priority_tls() {
        use zenoh_link_commons::tls::config::*;

        zenoh_util::init_log_from_env_or("error");

        // NOTE: this an auto-generated pair of certificate and key.
        //       The target domain is localhost, so it has no real
        //       mapping to any existing domain. The certificate and key
        //       have been generated using: https://github.com/jsha/minica
        let key = "-----BEGIN RSA PRIVATE KEY-----
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

        let cert = "-----BEGIN CERTIFICATE-----
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

        // Configure the client
        let ca = "-----BEGIN CERTIFICATE-----
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

        check(|| {
            let mut endpoint: EndPoint =
                format!("tls/localhost:{}", zenoh_test::get_free_tcp_port())
                    .parse()
                    .unwrap();
            endpoint
                .config_mut()
                .extend_from_iter(
                    [
                        (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                        (TLS_LISTEN_PRIVATE_KEY_RAW, key),
                        (TLS_LISTEN_CERTIFICATE_RAW, cert),
                    ]
                    .iter()
                    .copied(),
                )
                .unwrap();
            endpoint
        })
        .await;
    }
}
