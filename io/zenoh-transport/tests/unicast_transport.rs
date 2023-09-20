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
use async_std::{prelude::FutureExt, task};
use std::fmt::Write as _;
use std::{
    any::Any,
    convert::TryFrom,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use zenoh_core::zasync_executor_init;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{
        Channel, CongestionControl, Encoding, EndPoint, Priority, Reliability, WhatAmI, ZenohId,
    },
    network::{
        push::ext::{NodeIdType, QoSType},
        NetworkMessage, Push,
    },
    zenoh::Put,
};
use zenoh_result::ZResult;
use zenoh_transport::test_helpers::make_transport_manager_builder;
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

// These keys and certificates below are purposedly generated to run TLS and mTLS tests.
//
// With 2 way authentication (mTLS), using TLS 1.3, we need two pairs of keys and certificates: one
// for the "server" and another one for the "client".
//
// The keys and certificates below were auto-generated using https://github.com/jsha/minica and
// target the localhost domain, so it has no real mapping to any existing domain.
//
// The keys and certificates generated map as follows to the constants below:
//
//   certificates
//   ├── client
//   │   ├── localhost
//   │   │   ├── cert.pem <------- CLIENT_CERT
//   │   │   └── key.pem <-------- CLIENT_KEY
//   │   ├── minica-key.pem
//   │   └── minica.pem <--------- CLIENT_CA
//   └── server
//       ├── localhost
//       │   ├── cert.pem <------- SERVER_CERT
//       │   └── key.pem <-------- SERVER_KEY
//       ├── minica-key.pem
//       └── minica.pem <--------- SERVER_CA
//
// The way it works is that the client's certificate authority will validate in front of the server
// the key and certificate brought in by the client. Similarly the server's certificate authority
// will validate the key and certificate brought in by the server in front of the client.
//
#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const CLIENT_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
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

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const CLIENT_CERT: &str = "-----BEGIN CERTIFICATE-----
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

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const CLIENT_CA: &str = "-----BEGIN CERTIFICATE-----
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

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAmDCySqKHPmEZShDH3ldPaV/Zsh9+HlHFLk9H10vJZj5WfzVu
5puZQ8GvBFIOtVrl0L9qLkA6bZiHHXm/8OEVvd135ZMp4NV23fdTsEASXfvGVQY8
y+4UkZN0Dw6sfwlQVPyNRplys2+nFs6tX05Dp9VizV39tSOqe/jd6hyzxSUHqFat
RwQRXAI04CZ6ckDb0Riw7i0yvjrFhBom9lPKq4IkXZGgS5MRl0pRgAZTqHEMlv8z
oX+KcG9mfyQIHtpkVuSHHsQjwVop7fMnT7KCQ3bPI+fgMmAg+h1IR19Dm0JM+9zl
u39j0IbkytrsystGM+pTRbdp7s2lgtOMCFt0+wIDAQABAoIBADNTSO2uvlmlOXgn
DKDJZTiuYKaXxFrJTOx/REUxg+x9XYJtLMeM9jVJnpKgceFrlFHAHDkY5BuN8xNX
ugmsfz6W8BZ2eQsgMoRNIuYv1YHopUyLW/mSg1FNHzjsw/Pb2kGvIp4Kpgopv3oL
naCkrmBtsHJ+Hk/2hUpl9cE8iMwVWcVevLzyHi98jNy1IDdIPhRtl0dhMiqC5MRr
4gLJ5gNkLYX7xf3tw5Hmfk/bVNProqZXDIQVI7rFvItX586nvQ3LNQkmW/D2ShZf
3FEqMu6EdA2Ycc4UZgAlQNGV0VBrWWVXizOQ+9gjLnBk3kJjqfigCU6NG94bTJ+H
0YIhsGECgYEAwdSSyuMSOXgzZQ7Vv+GsNn/7ivi/H8eb/lDzksqS/JroA2ciAmHG
2OF30eUJKRg+STqBTpOfXgS4QUa8QLSwBSnwcw6579x9bYGUhqD2Ypaw9uCnOukA
CwwggZ9cDmF0tb5rYjqkW3bFPqkCnTGb0ylMFaYRhRDU20iG5t8PQckCgYEAyQEM
KK18FLQUKivGrQgP5Ib6IC3myzlHGxDzfobXGpaQntFnHY7Cxp/6BBtmASzt9Jxu
etnrevmzrbKqsLTJSg3ivbiq0YTLAJ1FsZrCp71dx49YR/5o9QFiq0nQoKnwUVeb
/hrDjMAokNkjFL5vouXO711GSS6YyM4WzAKZAqMCgYEAhqGxaG06jmJ4SFx6ibIl
nSFeRhQrJNbP+mCeHrrIR98NArgS/laN+Lz7LfaJW1r0gIa7pCmTi4l5thV80vDu
RlfwJOr4qaucD4Du+mg5WxdSSdiXL6sBlarRtVdMaMy2dTqTegJDgShJLxHTt/3q
P0yzBWJ5TtT3FG0XDqum/EkCgYAYNHwWWe3bQGQ9P9BI/fOL/YUZYu2sA1XAuKXZ
0rsMhJ0dwvG76XkjGhitbe82rQZqsnvLZ3qn8HHmtOFBLkQfGtT3K8nGOUuI42eF
H7HZKUCly2lCIizZdDVBkz4AWvaJlRc/3lE2Hd3Es6E52kTvROVKhdz06xuS8t5j
6twqKQKBgQC01AeiWL6Rzo+yZNzVgbpeeDogaZz5dtmURDgCYH8yFX5eoCKLHfnI
2nDIoqpaHY0LuX+dinuH+jP4tlyndbc2muXnHd9r0atytxA69ay3sSA5WFtfi4ef
ESElGO6qXEA821RpQp+2+uhL90+iC294cPqlS5LDmvTMypVDHzrxPQ==
-----END RSA PRIVATE KEY-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIW1mAtJWJAJYwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNGRjYzJmMCAXDTIzMDMwNjE2NDEwNloYDzIxMjMw
MzA2MTY0MTA2WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCYMLJKooc+YRlKEMfeV09pX9myH34eUcUuT0fXS8lm
PlZ/NW7mm5lDwa8EUg61WuXQv2ouQDptmIcdeb/w4RW93Xflkyng1Xbd91OwQBJd
+8ZVBjzL7hSRk3QPDqx/CVBU/I1GmXKzb6cWzq1fTkOn1WLNXf21I6p7+N3qHLPF
JQeoVq1HBBFcAjTgJnpyQNvRGLDuLTK+OsWEGib2U8qrgiRdkaBLkxGXSlGABlOo
cQyW/zOhf4pwb2Z/JAge2mRW5IcexCPBWint8ydPsoJDds8j5+AyYCD6HUhHX0Ob
Qkz73OW7f2PQhuTK2uzKy0Yz6lNFt2nuzaWC04wIW3T7AgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTX46+p+Po1npE6QLQ7mMI+83s6qDAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAAxrmQPG54ybKgMVliN8
Mg5povSdPIVVnlU/HOVG9yxzAOav/xQP003M4wqpatWxI8tR1PcLuZf0EPmcdJgb
tVl9nZMVZtveQnYMlU8PpkEVu56VM4Zr3rH9liPRlr0JEAXODdKw76kWKzmdqWZ/
rzhup3Ek7iEX6T5j/cPUvTWtMD4VEK2I7fgoKSHIX8MIVzqM7cuboGWPtS3eRNXl
MgvahA4TwLEXPEe+V1WAq6nSb4g2qSXWIDpIsy/O1WGS/zzRnKvXu9/9NkXWqZMl
C1LSpiiQUaRSglOvYf/Zx6r+4BOS4OaaArwHkecZQqBSCcBLEAyb/FaaXdBowI0U
PQ4=
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIITcwv1N10nqEwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNGRjYzJmMCAXDTIzMDMwNjE2NDEwNloYDzIxMjMw
MzA2MTY0MTA2WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSA0ZGNjMmYwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC2WUgN7NMlXIknew1cXiTWGmS0
1T1EjcNNDAq7DqZ7/ZVXrjD47yxTt5EOiOXK/cINKNw4Zq/MKQvq9qu+Oax4lwiV
Ha0i8ShGLSuYI1HBlXu4MmvdG+3/SjwYoGsGaShr0y/QGzD3cD+DQZg/RaaIPHlO
MdmiUXxkMcy4qa0hFJ1imlJdq/6Tlx46X+0vRCh8nkekvOZR+t7Z5U4jn4XE54Kl
0PiwcyX8vfDZ3epa/FSHZvVQieM/g5Yh9OjIKCkdWRg7tD0IEGsaW11tEPJ5SiQr
mDqdRneMzZKqY0xC+QqXSvIlzpOjiu8PYQx7xugaUFE/npKRQdvh8ojHJMdNAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTX46+p+Po1npE6
QLQ7mMI+83s6qDAfBgNVHSMEGDAWgBTX46+p+Po1npE6QLQ7mMI+83s6qDANBgkq
hkiG9w0BAQsFAAOCAQEAaN0IvEC677PL/JXzMrXcyBV88IvimlYN0zCt48GYlhmx
vL1YUDFLJEB7J+dyERGE5N6BKKDGblC4WiTFgDMLcHFsMGRc0v7zKPF1PSBwRYJi
ubAmkwdunGG5pDPUYtTEDPXMlgClZ0YyqSFJMOqA4IzQg6exVjXtUxPqzxNhyC7S
vlgUwPbX46uNi581a9+Ls2V3fg0ZnhkTSctYZHGZNeh0Nsf7Am8xdUDYG/bZcVef
jbQ9gpChosdjF0Bgblo7HSUct/2Va+YlYwW+WFjJX8k4oN6ZU5W5xhdfO8Czmgwk
US5kJ/+1M0uR8zUhZHL61FbsdPxEj+fYKrHv4woo+A==
-----END CERTIFICATE-----";

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_COUNT: Duration = Duration::from_millis(10);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];
const MSG_SIZE_LOWLATENCY: [usize; 2] = [1_024, 65000];
const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

// Transport Handler for the router
struct SHRouter {
    count: Arc<AtomicUsize>,
}

impl Default for SHRouter {
    fn default() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl SHRouter {
    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SCRouter::new(self.count.clone()));
        Ok(arc)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the router
pub struct SCRouter {
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

impl TransportPeerEventHandler for SCRouter {
    fn handle_message(&self, _message: NetworkMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Transport Handler for the client
#[derive(Default)]
struct SHClient;

impl TransportEventHandler for SHClient {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(SCClient))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the client
#[derive(Default)]
pub struct SCClient;

impl TransportPeerEventHandler for SCClient {
    fn handle_message(&self, _message: NetworkMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn open_transport_unicast(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    lowlatency_transport: bool,
) -> (
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
) {
    // Define client and router IDs
    let client_id = ZenohId::try_from([1]).unwrap();
    let router_id = ZenohId::try_from([2]).unwrap();

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::default());
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        server_endpoints.len(),
        #[cfg(feature = "shared-memory")]
        false,
        lowlatency_transport,
    );
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the listener on the router
    for e in server_endpoints.iter() {
        println!("Add endpoint: {}", e);
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create the client transport manager
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        client_endpoints.len(),
        #[cfg(feature = "shared-memory")]
        false,
        lowlatency_transport,
    );
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClient))
        .unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in client_endpoints.iter() {
        println!("Opening transport with {}", e);
        let _ = ztimeout!(client_manager.open_transport_unicast(e.clone())).unwrap();
    }

    let client_transport = client_manager
        .get_transport_unicast(&router_id)
        .await
        .unwrap();

    // Return the handlers
    (
        router_manager,
        router_handler,
        client_manager,
        client_transport,
    )
}

async fn close_transport(
    router_manager: TransportManager,
    client_manager: TransportManager,
    client_transport: TransportUnicast,
    endpoints: &[EndPoint],
) {
    // Close the client transport
    let mut ee = String::new();
    for e in endpoints.iter() {
        let _ = write!(ee, "{e} ");
    }
    println!("Closing transport with {}", ee);
    ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {}", e);
        ztimeout!(router_manager.del_listener(e)).unwrap();
    }

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn test_transport(
    router_handler: Arc<SHRouter>,
    client_transport: TransportUnicast,
    channel: Channel,
    msg_size: usize,
) {
    println!(
        "Sending {} messages... {:?} {}",
        MSG_COUNT, channel, msg_size
    );
    let cctrl = match channel.reliability {
        Reliability::Reliable => CongestionControl::Block,
        Reliability::BestEffort => CongestionControl::Drop,
    };
    // Create the message to send
    let message: NetworkMessage = Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(channel.priority, cctrl, false),
        ext_tstamp: None,
        ext_nodeid: NodeIdType::default(),
        payload: Put {
            payload: vec![0u8; msg_size].into(),
            timestamp: None,
            encoding: Encoding::default(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_unknown: vec![],
        }
        .into(),
    }
    .into();
    for _ in 0..MSG_COUNT {
        let _ = client_transport.schedule(message.clone());
        // print!("S-{i} ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
    }

    match channel.reliability {
        Reliability::Reliable => {
            ztimeout!(async {
                while router_handler.get_count() != MSG_COUNT {
                    task::sleep(SLEEP_COUNT).await;
                }
            });
        }
        Reliability::BestEffort => {
            ztimeout!(async {
                while router_handler.get_count() == 0 {
                    task::sleep(SLEEP_COUNT).await;
                }
            });
        }
    };

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run_single(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    channel: Channel,
    msg_size: usize,
    lowlatency_transport: bool,
) {
    println!(
        "\n>>> Running test for:  {:?}, {:?}, {:?}, {}",
        client_endpoints, server_endpoints, channel, msg_size
    );

    #[allow(unused_variables)] // Used when stats feature is enabled
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport_unicast(client_endpoints, server_endpoints, lowlatency_transport).await;

    test_transport(
        router_handler.clone(),
        client_transport.clone(),
        channel,
        msg_size,
    )
    .await;

    #[cfg(feature = "stats")]
    {
        let c_stats = client_transport.get_stats().unwrap().report();
        println!("\tClient: {:?}", c_stats);
        let r_stats = router_manager
            .get_transport_unicast(&client_manager.config.zid)
            .await
            .unwrap()
            .get_stats()
            .map(|s| s.report())
            .unwrap();
        println!("\tRouter: {:?}", r_stats);
    }

    close_transport(
        router_manager,
        client_manager,
        client_transport,
        client_endpoints,
    )
    .await;
}

async fn run_internal(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    channel: &[Channel],
    msg_size: &[usize],
    lowlatency_transport: bool,
) {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            run_single(
                client_endpoints,
                server_endpoints,
                *ch,
                *ms,
                lowlatency_transport,
            )
            .await;
        }
    }
}

async fn run_with_universal_transport(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    channel: &[Channel],
    msg_size: &[usize],
) {
    run_internal(client_endpoints, server_endpoints, channel, msg_size, false).await;
}

async fn run_with_lowlatency_transport(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    channel: &[Channel],
    msg_size: &[usize],
) {
    if client_endpoints.len() > 1 || server_endpoints.len() > 1 {
        println!("LowLatency transport doesn't support more than one link, so this test would produce MAX_LINKS error!");
        panic!();
    }
    run_internal(client_endpoints, server_endpoints, channel, msg_size, true).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_unicast_tcp_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 16000).parse().unwrap(),
        format!("tcp/[::1]:{}", 16001).parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_unicast_tcp_only_with_lowlatency_transport() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![format!("tcp/127.0.0.1:{}", 16100).parse().unwrap()];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
    ];
    // Run
    task::block_on(run_with_lowlatency_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_LOWLATENCY,
    ));
}

#[cfg(feature = "transport_udp")]
#[test]
fn transport_unicast_udp_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        format!("udp/127.0.0.1:{}", 16010).parse().unwrap(),
        format!("udp/[::1]:{}", 16011).parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_NOFRAG,
    ));
}

#[cfg(feature = "transport_udp")]
#[test]
fn transport_unicast_udp_only_with_lowlatency_transport() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![format!("udp/127.0.0.1:{}", 16110).parse().unwrap()];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_lowlatency_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_NOFRAG,
    ));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn transport_unicast_unix_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-5.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![format!("unixsock-stream/{f1}").parse().unwrap()];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn transport_unicast_unix_only_with_lowlatency_transport() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-5-lowlatency.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![format!("unixsock-stream/{f1}").parse().unwrap()];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_lowlatency_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_LOWLATENCY,
    ));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(feature = "transport_ws")]
#[test]
fn transport_unicast_ws_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("ws/127.0.0.1:{}", 16020).parse().unwrap(),
        format!("ws/[::1]:{}", 16021).parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_ws")]
#[test]
fn transport_unicast_ws_only_with_lowlatency_transport() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![format!("ws/127.0.0.1:{}", 16120).parse().unwrap()];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_lowlatency_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_LOWLATENCY,
    ));
}

#[cfg(feature = "transport_unixpipe")]
#[test]
fn transport_unicast_unixpipe_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "unixpipe/transport_unicast_unixpipe_only".parse().unwrap(),
        "unixpipe/transport_unicast_unixpipe_only2".parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_unixpipe")]
#[test]
fn transport_unicast_unixpipe_only_with_lowlatency_transport() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "unixpipe/transport_unicast_unixpipe_only_with_lowlatency_transport"
            .parse()
            .unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
    ];
    // Run
    task::block_on(run_with_lowlatency_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_LOWLATENCY,
    ));
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[test]
fn transport_unicast_tcp_udp() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 16030).parse().unwrap(),
        format!("udp/127.0.0.1:{}", 16031).parse().unwrap(),
        format!("tcp/[::1]:{}", 16032).parse().unwrap(),
        format!("udp/[::1]:{}", 16033).parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_NOFRAG,
    ));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_tcp_unix() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-6.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 16040).parse().unwrap(),
        format!("tcp/[::1]:{}", 16041).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_udp_unix() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-7.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        format!("udp/127.0.0.1:{}", 16050).parse().unwrap(),
        format!("udp/[::1]:{}", 16051).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_NOFRAG,
    ));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_tcp_udp_unix() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-8.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 16060).parse().unwrap(),
        format!("udp/127.0.0.1:{}", 16061).parse().unwrap(),
        format!("tcp/[::1]:{}", 16062).parse().unwrap(),
        format!("udp/[::1]:{}", 16063).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_NOFRAG,
    ));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only_server() {
    use zenoh_link::tls::config::*;

    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let mut endpoint: EndPoint = format!("tls/localhost:{}", 16070).parse().unwrap();
    endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    let endpoints = vec![endpoint];
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_quic")]
#[test]
fn transport_unicast_quic_only_server() {
    use zenoh_link::quic::config::*;

    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let mut endpoint: EndPoint = format!("quic/localhost:{}", 16080).parse().unwrap();
    endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    let endpoints = vec![endpoint];
    task::block_on(run_with_universal_transport(
        &endpoints,
        &endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only_mutual_success() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    let client_auth = "true";

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10461").parse().unwrap();
    client_endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                (TLS_CLIENT_CERTIFICATE_RAW, CLIENT_CERT),
                (TLS_CLIENT_PRIVATE_KEY_RAW, CLIENT_KEY),
                (TLS_CLIENT_AUTH, client_auth),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10461").parse().unwrap();
    server_endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
                (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_CLIENT_AUTH, client_auth),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    let client_endpoints = vec![client_endpoint];
    let server_endpoints = vec![server_endpoint];
    task::block_on(run_with_universal_transport(
        &client_endpoints,
        &server_endpoints,
        &channel,
        &MSG_SIZE_ALL,
    ));
}

// Constants replicating the alert descriptions thrown by the Rustls library.
// These alert descriptions are internal of the library and cannot be reached from these tests
// as to do a proper comparison. For the sake of simplicity we verify these constants are contained
// in the expected error messages from the tests below.
//
// See: https://docs.rs/rustls/latest/src/rustls/msgs/enums.rs.html#128
#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const RUSTLS_UNKNOWN_CA_ALERT_DESCRIPTION: &str = "UnknownCA";
#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const RUSTLS_CERTIFICATE_REQUIRED_ALERT_DESCRIPTION: &str = "CertificateRequired";

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only_mutual_no_client_certs_failure() {
    use std::vec;

    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10462").parse().unwrap();
    client_endpoint
        .config_mut()
        .extend(
            [(TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA)]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10462").parse().unwrap();
    server_endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
                (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_CLIENT_AUTH, "true"),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    let client_endpoints = vec![client_endpoint];
    let server_endpoints = vec![server_endpoint];
    let result = std::panic::catch_unwind(|| {
        task::block_on(run_with_universal_transport(
            &client_endpoints,
            &server_endpoints,
            &channel,
            &MSG_SIZE_ALL,
        ))
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    let error_msg = panic_message::panic_message(&err);
    assert!(error_msg.contains(RUSTLS_CERTIFICATE_REQUIRED_ALERT_DESCRIPTION));
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only_mutual_wrong_client_certs_failure() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    let client_auth = "true";

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10463").parse().unwrap();
    client_endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                // Using the SERVER_CERT and SERVER_KEY in the client to simulate the case the client has
                // wrong certificates and keys. The SERVER_CA (cetificate authority) will not recognize
                // these certificates as it is expecting to receive CLIENT_CERT and CLIENT_KEY from the
                // client.
                (TLS_CLIENT_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_CLIENT_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_CLIENT_AUTH, client_auth),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10463").parse().unwrap();
    server_endpoint
        .config_mut()
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
                (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_CLIENT_AUTH, client_auth),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    let client_endpoints = vec![client_endpoint];
    let server_endpoints = vec![server_endpoint];
    let result = std::panic::catch_unwind(|| {
        task::block_on(run_with_universal_transport(
            &client_endpoints,
            &server_endpoints,
            &channel,
            &MSG_SIZE_ALL,
        ))
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    let error_msg = panic_message::panic_message(&err);
    assert!(error_msg.contains(RUSTLS_UNKNOWN_CA_ALERT_DESCRIPTION));
}

#[test]
fn transport_unicast_qos_and_lowlatency_failure() {
    struct TestPeer;
    impl TransportEventHandler for TestPeer {
        fn new_unicast(
            &self,
            _: TransportPeer,
            _: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            panic!();
        }

        fn new_multicast(
            &self,
            _: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    let peer_shm02_handler = Arc::new(TestPeer);

    let failing_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .unicast(
            TransportManager::config_unicast()
                .lowlatency(true)
                .qos(true),
        )
        .build(peer_shm02_handler.clone());
    assert!(failing_manager.is_err());

    let good_manager1 = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .unicast(
            TransportManager::config_unicast()
                .lowlatency(false)
                .qos(true),
        )
        .build(peer_shm02_handler.clone());
    assert!(good_manager1.is_ok());

    let good_manager2 = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .unicast(
            TransportManager::config_unicast()
                .lowlatency(true)
                .qos(false),
        )
        .build(peer_shm02_handler.clone());
    assert!(good_manager2.is_ok());
}
