//
// Copyright (c) 2022 ZettaScale Technology
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
use zenoh_buffers::ZBuf;
use zenoh_core::{zasync_executor_init, Result as ZResult};
use zenoh_link::Link;
use zenoh_protocol::{
    core::{Channel, CongestionControl, EndPoint, Priority, Reliability, WhatAmI, ZenohId},
    zenoh::ZenohMessage,
};
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
MIIEogIBAAKCAQEAxeTx0qv8qwy3cWa/MJ+2RegsrvJxfw3A5AKHVe8vLj0Uz5ip
ZVA+ydp1NmaXtkxrALazbE7sOpvFKZX7Vf02jL/5TEVPf0NAXq0YKe81x5ZSkcLF
QrAopS+pC9O7LpOotWqtaYXx3We9fpsg+yaihNqLpKLIeg3vAFB8QCejfrw5IJG0
J/fvrQLvHs/TkO/Ckb6p6ZnL61aZPWdwmMUUglbUIsjNU4giA2BRTBaaDfDH1G/j
zfC7lpwH3H9S5siwzSU5YZZR8QgfMPomIbmESaHWjx3t4MB9MzAKn2wASejJYgBC
r0NeKkwLLyEC/X9B5in40g66x/JETwRLCpscfQIDAQABAoIBADRt0o+hF0DuDo/R
y+eC+NSOjYAQJXem2irObLKcuuBCOIhDhuWbm/b4lMND7P/UQSkgPmr8geOJL3Q0
EzGV82TY26CUYFp0I9Kxg0xg3tuw/NE3S/G+IBabiOrkPpw5bKIb0DO70/d3q6Gm
UdeYRchy6jpFEl4b4O0xZanNlqhVl5lres7vaeUOQ/jNdejDMDWuvh/jyL3CSlWg
Nw/6BRz2zs7GEJZZ0yHHM/jzbIysntEYxA5U/yG1j1SX5ZSfrds+ckrYRxVZ30gA
YCSz7B60Vv0ymcwnjeFRTTQIqxcunT4i7Jwva4wG0Dd9NbOUTyvtd2ylGxneKGB1
1lS/nGECgYEA/k9R7KmMu/nu/0fKgLl/Q+boG/GAsRrMCDJN6MS/Ep2g4q2EZ8C6
xJGKUTvwh4pjZqeix9Ix03NHKlB6CjlCN67mY/mBchTo//Qj6ptIlBg1DrHq5DSf
1dyD8avlgUVcSU2Obdd4L3ZvXG3+Bf6mx+72OkJ34AU24z7PJTTCMqkCgYEAxzWj
zJbRGaeazL+Kvwthjz3SWvmeOA8GnwWbWyatVe81qLelgpkmmnJJvonaxHFmX93x
FRizwOTSTocm2nk5eQ9wIS+4ubzDAcfvz6vy7Ib+B0Fcg0FFyxR0TGrozqrDIqLW
9tdKvJebc5yfzuEfHluR4S2bi/+me7ng3ASs07UCgYBcowZDwGtsmhmuUjd49ple
YcGRVEK9wPYr0i9BKFI19MeDaxO9O56NNjr9Zmky5n1ZCp2oTnAqB2cYCeK60KrH
X+W660t1BBrwCb3/mvswPzUsmjDnWigTHlXN9gEPOvXoGeFVL9Uu7OSZ9dM/2chl
Mi3tgQLrztp0ow+QDQzkqQKBgB8fe0LYgTyv2diJSGUGoyxc7UN3YkfB2Tf5CUeZ
aFVXtRtx7bLUuJpCptDU+s/cI7FwnFy+aj8FwPGx3dkePWNzjQIyUXr7ScA6e3YH
mEFp6cA6bvi2tu++d1kFDvBS73+2zzzrb+q9CPVsD++jblgw2D7FAFtECr+jz8Sw
GkxNAoGAGACihffFDmnhILYyKTIi/N6DMuFdy7kaXWEpgbZUwwoFFTNCMGvZYw8Y
d1lYvXgMtS1d2TfGzbfh+jYoeOoTj1KuAc5MIBHm85huxMHffEo+uCH1om7R1r4M
aEp+YSTk9DX5QnrIA5BOdr4XnkCJ9cdGtDaSH7WsoqJFeva0Bnw=
-----END RSA PRIVATE KEY-----";

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const CLIENT_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIOSrMNOHTWkEwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNjVhMmE5MB4XDTIyMTEyNTA5NTMyOVoXDTI0MTIy
NTA5NTMyOVowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAxeTx0qv8qwy3cWa/MJ+2RegsrvJxfw3A5AKHVe8vLj0U
z5ipZVA+ydp1NmaXtkxrALazbE7sOpvFKZX7Vf02jL/5TEVPf0NAXq0YKe81x5ZS
kcLFQrAopS+pC9O7LpOotWqtaYXx3We9fpsg+yaihNqLpKLIeg3vAFB8QCejfrw5
IJG0J/fvrQLvHs/TkO/Ckb6p6ZnL61aZPWdwmMUUglbUIsjNU4giA2BRTBaaDfDH
1G/jzfC7lpwH3H9S5siwzSU5YZZR8QgfMPomIbmESaHWjx3t4MB9MzAKn2wASejJ
YgBCr0NeKkwLLyEC/X9B5in40g66x/JETwRLCpscfQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAUPGIecA4Wf8IBsvT/CbyLOBxaj7wwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQACkX6QryVOwVxlm/d8zPyU
eVOquvwATtGHR1Ra32abgL4o0DSTEs2zsPGLlsyefbs9VVq0l6UOCfnaLBJ02izx
UjEQcvSuMKjexDPmTEUa3ZJi8xV5Rx+/jOQDSHuMzdSp27OIn3kP/Ym8rVKW/GPD
ISVQ1D3DTCfe9vo6BO8+k4+JjVLwS0mqSEcNzIe3VqpYOa2Ic6uHsfw1+YFGFPIG
WUswTbYMCsLT9fcAl3EMTE7Diub9LfPPC51U4EUyTdnWegK6WWKJwwUfFij2Hw8V
ob5ssEzyjB+/+toNeOgNc8LCPV6iECtY1uuaKRkYHLFVvIGyr0WjBGZbXN+DFgjU
-----END CERTIFICATE-----";

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
const CLIENT_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIZaKp4MoEShowDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNjVhMmE5MCAXDTIyMTEyNTA5NTMyOVoYDzIxMjIx
MTI1MDk1MzI5WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSA2NWEyYTkwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC46jtwefctdTkSSRngC0fg8Du1
i5HliIwu0UcBQx7iayY7dLAsbWnZjc2FW5MpCyLzeMyJwDGLP6wvH5u/26D7ZXe+
EzW39EaIOG05SSDBgAmbP84yxbUWEtJRK64XYdMx3UkM611hWvd77X6UOrXN6cCW
FmEsYl/TDDJMGpGFzoMNW8EJuraysaKcerfMtwEhDDx7OacUXnAczUmpUkHhLSf/
FjAA+SkGyiRZMFK7HdMo6wDKKGzFV3JRDt7U7INLRvWCUDMwGRKz6e4B4hgmTEM1
az8WtqEMHIu/M290u2dyo/KuXO2qt7nReqNQPFlah/WXeoTfmfI4H1vlpNHZAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQ8Yh5wDhZ/wgGy
9P8JvIs4HFqPvDAfBgNVHSMEGDAWgBQ8Yh5wDhZ/wgGy9P8JvIs4HFqPvDANBgkq
hkiG9w0BAQsFAAOCAQEAplVD4isp0ePjhdOT/dYgjs2iWp5SpIRShUbA5CgBs0f9
oxgWKkrFhtOrimuPdeTWSVYKggC8ZnjFxLKJQp8BJ/U/cpHf0FUFTyFs6SuphEzD
UkT4PD+tl6diP7QhiKcmoUpjMWyclboHBPfjv9L8hevCvmila9jViTXgPUmCTzZR
PXe2XrD6m3LNYrN3MEVapbcRR4GgJCLCm+gIV69TQR+CTd3HONDUBb78ezoifJMe
+s/+SotAlLpjjUE8qIkeObmcwYqS9gW1QI8e6QqEbnCzm9gwx7Gv4iF3zXNcBm6A
pl2wSO0VV8S+/zZRNE6OCSjOeEe2kj7vC/jsvoRyGg==
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAzioJ1vjs/5RjuuF0X8dtABcw56fFKo3zJb8TFe+fVWIJ1cde
qp/XYyOR/YhFtv4rhVaoqM5jDOKCQTOsEVZtbGmQpxoaGxPYCeN5v9e/aM+5/F9e
7bQil40pC3tFFU0xA3MoPCJQoWUcKdPx280XDaeAFu3cT2lgLiCKeGhj/jBA0MIK
yYYpeVpsqIwM1/OXJk2Q+VuSlFo2XsUfTmI59noK+7dnbMgLe06zsDOpha7h+vxS
+6FFSmjP54cC6zWaqGSIu8Z1GfdTzClbEu4PP+xW8k8WbT899a0AZ5atLrnQJZnu
IkkM9KcOwqyPbPmEhdvSkXPVTMPZ6J6Iy2XGAwIDAQABAoIBAG1P8umuEROu3XZQ
ZoEGX7Alm8OovC1FGMBR4M/HvybpladpT7OOjwi86oKQh85wVCkd+s2OqQxC6aei
u0ByIBDUpGNFvsPOsMUFfR+QSKhNRCMhelb1JUZQDYxN7yJNTIi0vYes9i2tS9KN
03ak9u1Be/QsaJ0elWW/0UvopeQ3Qr8LsdccRoo3M22cp2xd2ubf0TeULMwRL3jE
9jTGdy/8O2+5XJu/4NKUwad7YB81rb4QWqJGvPC/+uwYjH8Zz42jc2GHm5/Akixe
SW0EUCHUNQV3dOD5cmJATwxXyrhMNhgW4y9MjtiTHVC3x2lwzjC5x9bsuQFrY4a7
xP9hC/ECgYEAz9/Y8y1CawAmnJG3OmuWZs0IoeuPMMvOEL8YqQTAPB4dXsTjb/Hw
wu3ueQuPR7PsQIVpmL1aC3vZuWn2YoIZfX7huUiqmXb2i4kYG1EGvXLUR35pHEGa
qpS7Fpuj9kajwY4zF5BI5U4iloRjf1VPks+JuuWH2lWYO7V8RSUJUrkCgYEA/eTV
IrMJGNPZbaTNbrdwtbIPUpaPBLo/fPYUVMxz/7pSuzqK4eh5jgGNFA9NlAll8gGA
5NmcdjaxmQzM5IPKj/14ZoUZcqFq7aLFLX37DhEazzTcnG96Fj43+OyG1HZ6dAUK
D+C0ltsrqO8PB/cC3FQyv6L9HAGIc5C5OdIuMJsCgYBzxeP6a7aWCVt3z+AQdWMq
lf68z4jMUHXP9d4yJCc8VDlfUqCo9EJ3DjTGzZ1a/eYSeTs6ihrgUnYMQeurKXIw
5r2oh8Qb/JmLVStL63Cpio6X0tuPlSoi3vrjuIM04lrJrfzensk6jK3OzqTrggPz
bAr1QGjNPOawOn+fsuTiYQKBgQCsWFSRzGSFdPEoK3HEEUOyIt+h2U/WDrOgGM7u
TScE1a7pJzE1boBs9AKXNlgcAFEyePDM6Cb8W94snXLMP+YV3iKHvRvsI0SZcR9V
5Smxf8zqEOEcU9PVG4EVOUHBIXe4H9+XrZoIuVgmwbg7WOKZO5KDYZldFHFSuU/y
vwjZtwKBgHOjPzT4KQjJcysHPaEjcC423iBAlQGgi+winoF7D/+W37vmLefQH6eK
kRJuxLCQuudMXhaiINIiZL/WQq+lFKXX5VmoTNcznvHUZAQQqj6+WAkrUMXn2cAP
TQwa4AaQu4QOkexzToUuFSn9wny0kUqrw/5+qvC/M1M3OuUCtln2
-----END RSA PRIVATE KEY-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIXeOjaf4eMxEwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgM2IxODk0MB4XDTIyMTEyNTA5NTMzN1oXDTI0MTIy
NTA5NTMzN1owFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAzioJ1vjs/5RjuuF0X8dtABcw56fFKo3zJb8TFe+fVWIJ
1cdeqp/XYyOR/YhFtv4rhVaoqM5jDOKCQTOsEVZtbGmQpxoaGxPYCeN5v9e/aM+5
/F9e7bQil40pC3tFFU0xA3MoPCJQoWUcKdPx280XDaeAFu3cT2lgLiCKeGhj/jBA
0MIKyYYpeVpsqIwM1/OXJk2Q+VuSlFo2XsUfTmI59noK+7dnbMgLe06zsDOpha7h
+vxS+6FFSmjP54cC6zWaqGSIu8Z1GfdTzClbEu4PP+xW8k8WbT899a0AZ5atLrnQ
JZnuIkkM9KcOwqyPbPmEhdvSkXPVTMPZ6J6Iy2XGAwIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAUBur8rCz010AuSG2z7fCJCy0vZNkwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQDQYQsZYpYKXUNNHA7VSvHm
+Z+Zvjkwj/L3NOZITAVu1z6cbvq9vPwGXSFrBUxbqqDxhcQHUUny/se6wU6/443K
PrwIPBf2ChjyqjE6TEH8RyyZrcvQfad7qDSSVU5YUVBUmRoL20NNDFRBcTA49Dgq
GreUwOidEQC4enZ4YDj7ZSZSZJzCP7Ouot+gXpLV5GIEnqSqgK/M39DF5K7aUWjq
qFHVJDC8rBEO/OU2q7S/mXazPeOlbERfGg4HwKPIYk8ApUYGS9P3f+E1P5Xu1LCp
pUgoglAG8l3GB7McegYSZy9PRyhAqcMxXsbtlIkjKG/kfgAEIjuHVrbWRMyMEpDo
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIOxiUVrnjG2MwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgM2IxODk0MCAXDTIyMTEyNTA5NTMzN1oYDzIxMjIx
MTI1MDk1MzM3WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAzYjE4OTQwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDgEzDNj2+IkmaMKhsd/rUOIer8
JZYU8Jg6YcZflnSAVznBqbv4+v+Z1Xl79XJQTiZ+AO7anFO6YoUztJVb/BuCdWSD
PdxFAWYORy9iKcBKVJArpuRHGYACM9YzFG3l0uiNvLKA6EtoLJVh6/enO/n4LjJl
mz8Z/IQ/Yw801Kv9uTLFbXg//1YATk336zGzpOl6JXiuSl4EMPtLhrguYXB0wM7Z
Q6t4dcv20K58FwOcGGbTLUoZAZY1v55Y1Z0B1eRzb6DD1+0z16zvwiUSRkyFmBiU
lmUkaW4+6Hwh9SBO6Crt8Cpjq0DueYMfc47QgHrlhgPk0zytTbv+UU6qQ2MlAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQG6vysLPTXQC5I
bbPt8IkLLS9k2TAfBgNVHSMEGDAWgBQG6vysLPTXQC5IbbPt8IkLLS9k2TANBgkq
hkiG9w0BAQsFAAOCAQEA3z+lrAHVw6h9NXA+p5KOizyvyGgkUuMmvRjkAG8U+I8d
6mXYivnsooPaEHc8aaDLHbPiK8TlegC3enuxBo0M683fblVfKwQg1xBzD+s2E/4j
D3ZcoJDYwMCvoUPWyCyfrAmXmpk63BAVFQk4sWYkUrAvT9IJER8cTX/T7GZp4Wed
q6UXlWEPeROzBsrYBFJCEUzPhex/KHVSyezMcKFicBcnX6AZ4LTo3HR2hFn+X4AP
zZPoFKzCwOPYZ9gg9sIzFjB6Y6ykBLVVbrwmYdx0yqf4l4svzbVy/Ycv4uqVyyDm
KhZirNrBdkwVQK9j+lzGd+JeuXZhHIISCZKGRUDaWA==
-----END CERTIFICATE-----";

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_COUNT: Duration = Duration::from_millis(10);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];
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
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
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
        Ok(Arc::new(SCClient::default()))
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
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
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

async fn open_transport(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
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
    let unicast = TransportManager::config_unicast().max_links(server_endpoints.len());

    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the listener on the router
    for e in server_endpoints.iter() {
        println!("Add endpoint: {}\n", e);
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create the client transport manager
    let unicast = TransportManager::config_unicast().max_links(client_endpoints.len());
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClient::default()))
        .unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in client_endpoints.iter() {
        println!("Opening transport with {}", e);
        let _ = ztimeout!(client_manager.open_transport(e.clone())).unwrap();
    }

    let client_transport = client_manager.get_transport(&router_id).unwrap();

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
        let _ = write!(ee, "{} ", e);
    }
    println!("Closing transport with {}", ee);
    ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
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
    // Create the message to send
    let key = "test".into();
    let payload = ZBuf::from(vec![0_u8; msg_size]);
    let data_info = None;
    let routing_context = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        channel,
        CongestionControl::Block,
        data_info,
        routing_context,
        reply_context,
        attachment,
    );

    println!(
        "Sending {} messages... {:?} {}",
        MSG_COUNT, channel, msg_size
    );
    for _ in 0..MSG_COUNT {
        client_transport.schedule(message.clone()).unwrap();
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
) {
    #[allow(unused_variables)] // Used when stats feature is enabled
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport(client_endpoints, server_endpoints).await;

    test_transport(
        router_handler.clone(),
        client_transport.clone(),
        channel,
        msg_size,
    )
    .await;

    #[cfg(feature = "stats")]
    {
        let c_stats = client_transport.get_stats().unwrap();
        println!("\tClient: {:?}", c_stats,);
        let r_stats = router_manager
            .get_transport_unicast(&client_manager.config.zid)
            .unwrap()
            .get_stats()
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

async fn run(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
    channel: &[Channel],
    msg_size: &[usize],
) {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            run_single(client_endpoints, server_endpoints, *ch, *ms).await;
        }
    }
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG));
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
    let endpoints: Vec<EndPoint> = vec![format!("unixsock-stream/{}", f1).parse().unwrap()];
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{}.lock", f1));
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG));
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
        format!("unixsock-stream/{}", f1).parse().unwrap(),
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{}.lock", f1));
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
        format!("unixsock-stream/{}", f1).parse().unwrap(),
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{}.lock", f1));
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
        format!("unixsock-stream/{}", f1).parse().unwrap(),
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{}.lock", f1));
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
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
    task::block_on(run(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL));
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
    task::block_on(run(
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
const RUSTLS_HANDSHAKE_FAILURE_ALERT_DESCRIPTION: &str = "HandshakeFailure";
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
        task::block_on(run(
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
        task::block_on(run(
            &client_endpoints,
            &server_endpoints,
            &channel,
            &MSG_SIZE_ALL,
        ))
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    let error_msg = panic_message::panic_message(&err);
    assert!(error_msg.contains(RUSTLS_HANDSHAKE_FAILURE_ALERT_DESCRIPTION));
}
