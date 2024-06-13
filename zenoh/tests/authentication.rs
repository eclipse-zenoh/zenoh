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
mod test {
    use std::{
        fs,
        path::Path,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use tokio::runtime::Handle;
    use zenoh::{
        config,
        config::{EndPoint, WhatAmI},
        prelude::*,
        Config, Session,
    };
    use zenoh_core::{zlock, ztimeout};

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const KEY_EXPR: &str = "test/demo";
    const VALUE: &str = "zenoh";
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_authentication() {
        zenoh_util::try_init_log_from_env();
        let path = "./tests/testfiles";
        create_new_files(path).await.unwrap();
        println!("testfiles created successfully.");

        test_pub_sub_deny_then_allow_usrpswd().await;
        test_pub_sub_allow_then_deny_usrpswd().await;
        test_get_qbl_allow_then_deny_usrpswd().await;
        test_get_qbl_deny_then_allow_usrpswd().await;

        test_pub_sub_deny_then_allow_tls(3774).await;
        test_pub_sub_allow_then_deny_tls(3775).await;
        test_get_qbl_allow_then_deny_tls(3776).await;
        test_get_qbl_deny_then_allow_tls(3777).await;

        test_pub_sub_deny_then_allow_quic(3774).await;
        test_pub_sub_allow_then_deny_quic(3775).await;
        test_get_qbl_deny_then_allow_quic(3776).await;
        test_get_qbl_allow_then_deny_quic(3777).await;

        std::fs::remove_dir_all(path).unwrap();
        println!("testfiles removed successfully.");
    }

    #[allow(clippy::all)]
    async fn create_new_files(file_path: &str) -> std::io::Result<()> {
        use std::io::prelude::*;
        let ca_pem = b"-----BEGIN CERTIFICATE-----
MIIDiTCCAnGgAwIBAgIUO1x6LAlICgKs5+pYUTo4CughfKEwDQYJKoZIhvcNAQEL
BQAwVDELMAkGA1UEBhMCRlIxCzAJBgNVBAgMAklGMQswCQYDVQQHDAJQUjERMA8G
A1UECgwIenMsIEluYy4xGDAWBgNVBAMMD3pzX3Rlc3Rfcm9vdF9jYTAeFw0yNDAz
MTExNDM0MjNaFw0yNTAzMTExNDM0MjNaMFQxCzAJBgNVBAYTAkZSMQswCQYDVQQI
DAJJRjELMAkGA1UEBwwCUFIxETAPBgNVBAoMCHpzLCBJbmMuMRgwFgYDVQQDDA96
c190ZXN0X3Jvb3RfY2EwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC3
pFWM+IJNsRCYHt1v/TliecppwVZV+ZHfFw9JKN9ev4K/fWHUiAOwp91MOLxbaYKd
C6dxW28YVGltoGz3kUZJZcJRQVso1jXv24Op4muOsiYXukLc4TU2F6dG1XqkLt5t
svsYAQFf1uK3//QZFVRBosJEn+jjiJ4XCvt49mnPRolp1pNKX0z31mZO6bSly6c9
OVlJMjWpDCYSOuf6qZZ36fa9eSut2bRJIPY0QCsgnqYBTnIEhksS+3jy6Qt+QpLz
95pFdLbW/MW4XKpaDltyYkO6QrBekF6uWRlvyAHU+NqvXZ4F/3Z5l26qLuBcsLPJ
kyawkO+yNIDxORmQgMczAgMBAAGjUzBRMB0GA1UdDgQWBBThgotd9ws2ryEEaKp2
+RMOWV8D7jAfBgNVHSMEGDAWgBThgotd9ws2ryEEaKp2+RMOWV8D7jAPBgNVHRMB
Af8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQA9QoPv78hGmvmqF4GZeqrOBKQB
N/H5wL7f8H6BXU/wpNo2nnWOJn3u37lT+zivAdGEv+x+GeKekcugKBCSluhBLpVb
VNXe4WwMm5FBuO2NRBN2nblTMm1kEO00nVk1/yNo4hI8mj7d4YLU62d7324osNpF
wHqu6B0/c99JeKRvODGswyff1i8rJ1jpcgk/JmHg7UQBHEIkn0cRR0f9W3Mxv6b5
ZeowRe81neWNkC6IMiMmzA0iHGkhoUMA15qG1ZKOr1XR364LH5BfNNpzAWYwkvJs
0JFrrdw+rm+cRJWs55yiyCCs7pyg1IJkY/o8bifdCOUgIyonzffwREk3+kZR
-----END CERTIFICATE-----";

        let client_side_pem = b"-----BEGIN CERTIFICATE-----
MIIDjDCCAnSgAwIBAgIUOi9jKILrOzfRNGIkQ48S90NehpkwDQYJKoZIhvcNAQEL
BQAwVDELMAkGA1UEBhMCRlIxCzAJBgNVBAgMAklGMQswCQYDVQQHDAJQUjERMA8G
A1UECgwIenMsIEluYy4xGDAWBgNVBAMMD3pzX3Rlc3Rfcm9vdF9jYTAeFw0yNDAz
MTkxMTMxNDhaFw0yNTAzMTkxMTMxNDhaMFAxCzAJBgNVBAYTAkZSMQswCQYDVQQI
DAJJRjELMAkGA1UEBwwCUFIxETAPBgNVBAoMCHpzLCBJbmMuMRQwEgYDVQQDDAtj
bGllbnRfc2lkZTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMzU2p1a
ly/1bi2TDZ8+Qlvk9/3KyHqrg2BGZUxB3Pj/lufDuYNwOHkss99wp8gzMsT28mD4
y6X7nCgEN8WeHl+/xfLuGsWIBa1OOr6dz0qewoWFsor01cQ8+nwAKlgnz6IvHfkQ
OJZD/QYSdyn6c1AcIyS60vo4qMjyI4OVb1Dl4WpC4vCmWvDT0WjBZ5GckCnuQ8wS
wZ5MtPuMQf8kYX95ll7eBtDfEXF9Oja0l1/5SmlHuKyqDy4sIKovxtFHTqgb8PUc
yT33pUHOsBXruNBxl1MKq1outdMqcQknT6FAC+aVZ7bTlwhnH8p5Apn57g+dJYTI
9dCr1e2oK5NohhkCAwEAAaNaMFgwFgYDVR0RBA8wDYILY2xpZW50X3NpZGUwHQYD
VR0OBBYEFHDUYYfQacLj1tp49OG9NbPuL0N/MB8GA1UdIwQYMBaAFOGCi133Czav
IQRoqnb5Ew5ZXwPuMA0GCSqGSIb3DQEBCwUAA4IBAQB+nFAe6QyD2AaFdgrFOyEE
MeYb97sy9p5ylhMYyU62AYsIzzpTY74wBG78qYPIw3lAYzNcN0L6T6kBQ4lu6gFm
XB0SqCZ2AkwvV8tTlbLkZeoO6rONeke6c8cJsxYN7NiknDvTMrkTTgiyvbCWfEVX
Htnc4j/KzSBX3UjVcbPM3L/6KwMRw050/6RCiOIPFjTOCfTGoDx5fIyBk3ch/Plw
TkH2juHxX0/aCxr8hRE1v9+pXXlGnGoKbsDMLN9Aziu6xzdT/kD7BvyoM8rh7CE5
ae7/R4sd13cZ2WGDPimqO0z1kItMOIdiYvk4DgOg+J8hZSkKT56erafdDa2LPBE6
-----END CERTIFICATE-----";

        let client_side_key = b"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDM1NqdWpcv9W4t
kw2fPkJb5Pf9ysh6q4NgRmVMQdz4/5bnw7mDcDh5LLPfcKfIMzLE9vJg+Mul+5wo
BDfFnh5fv8Xy7hrFiAWtTjq+nc9KnsKFhbKK9NXEPPp8ACpYJ8+iLx35EDiWQ/0G
Encp+nNQHCMkutL6OKjI8iODlW9Q5eFqQuLwplrw09FowWeRnJAp7kPMEsGeTLT7
jEH/JGF/eZZe3gbQ3xFxfTo2tJdf+UppR7isqg8uLCCqL8bRR06oG/D1HMk996VB
zrAV67jQcZdTCqtaLrXTKnEJJ0+hQAvmlWe205cIZx/KeQKZ+e4PnSWEyPXQq9Xt
qCuTaIYZAgMBAAECggEAAlqVVw7UEzLjtN4eX1S6tD3jvCzFBETdjgENF7TfjlR4
lln9UyV6Xqkc+Y28vdwZwqHwW90sEPCc5ShUQD7+jBzi8FVcZSX4o7rVCbz8RXgg
1eI5EKf632YQflWNpwTxGcTnGCY/sjleil/yst6sDdD+9eR4OXQme2Wt8wyH8pLm
bf1OensGrFu3kJaPMOfP6jXnqEqkUPqmaCNW7+Ans8E+4J9oksRVPQJEuxwSjdJu
BlG50KKpl0XwZ/u/hkkj8/BlRDa62YMGJkFOwaaGUu2/0UU139XaJiMSPoL6t/BU
1H15dtW9liEtnHIssXMRzc9cg+xPgCs79ABXSZaFUQKBgQD4mH/DcEFwkZQcr08i
GUk0RE5arAqHui4eiujcPZVV6j/L7PHHmabKRPBlsndFP7KUCtvzNRmHq7JWDkpF
S36OE4e94CBYb0CIrO8OO5zl1vGAn5qa9ckefSFz9AMWW+hSuo185hFjt67BMaI0
8CxfYDH+QY5D4JE5RhSwsOmiUQKBgQDS7qjq+MQKPHHTztyHK8IbAfEGlrBdCAjf
K1bDX2BdfbRJMZ+y8LgK5HxDPlNx2/VauBLsIyU1Zirepd8hOsbCVoK1fOq+T7bY
KdB1oqLK1Rq1sMBc26F24LBaZ3Pw5XgYEcvaOW0JFQ9Oc4VjcIXKjTNhobNOegfK
QDnw8fEtSQKBgQDrCuTh2GVHFZ3AcVCUoOvB60NaH4flRHcOkbARbHihvtWK7gC8
A97bJ8tTnCWA5/TkXFAR54a36/K1wtUeJ38Evhp9wEdU1ftiPn/YKSzzcwLr5fu7
v9/kX9MdWv0ASu2iKphUGwMeETG9oDwJaXvKwZ0DFOB59P3Z9RTi6qI7wQKBgQCp
uBZ6WgeDJPeBsaSHrpHUIU/KOV1WvaxFxR1evlNPZmG1sxQIat/rA8VoZbHGn3Ff
uVSgY/cAbGB6HYTXu+9JV0p8tTI8Ru+cJqjwvhe2lJmVL87X6HCWsluzoiIL5tcm
pssbn7E36ZYTTag6RsOgItUA7ZbUwiOafOsiD8o64QKBgE6nOkAfy5mbp7X+q9uD
J5y6IXpY/Oia/RwveLWFbI/aum4Nnhb6L9Y0XlrYjm4cJOchQyDR7FF6f4EuAiYb
wdxBbkxXpwXnfKCtNvMF/wZMvPVaS5HTQga8hXMrtlW6jtTJ4HmkTTB/MILAXVkJ
EHi+N70PcrYg6li415TGfgDz
-----END PRIVATE KEY-----";

        let server_side_pem = b"-----BEGIN CERTIFICATE-----
MIIDjDCCAnSgAwIBAgIUOi9jKILrOzfRNGIkQ48S90NehpgwDQYJKoZIhvcNAQEL
BQAwVDELMAkGA1UEBhMCRlIxCzAJBgNVBAgMAklGMQswCQYDVQQHDAJQUjERMA8G
A1UECgwIenMsIEluYy4xGDAWBgNVBAMMD3pzX3Rlc3Rfcm9vdF9jYTAeFw0yNDAz
MTkxMTMxMDRaFw0yNTAzMTkxMTMxMDRaMFAxCzAJBgNVBAYTAkZSMQswCQYDVQQI
DAJJRjELMAkGA1UEBwwCUFIxETAPBgNVBAoMCHpzLCBJbmMuMRQwEgYDVQQDDAtz
ZXJ2ZXJfc2lkZTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKw4eKzt
T1inzuEIPBaPksWyjoD9n6uJx9jAQ2wRB6rXiAsXVLRSuczdGDpb1MwAqoIi6ozw
tzDRwkr58vUNaTCswxadlAmB44JEVYKZoublHjlVj5ygr0R4R5F2T9tIV+jpqZuK
HR4dHe8PiDCiWVzWvYwOLVKXQKSeaE2Z143ukVIJ85qmNykJ066AVhgWnIYSCR9c
s7WPBdTWAW3L4yNlast9hfvxdQNDs5AtUnJKfAX+7DylPAm8V7YjU1k9AtTNPbpy
kb9X97ErsB8891MmZaGZp0J6tnuucDkk0dlowMVvi2aUCsYoKF5DgGxtyVAeLhTP
70GenaLe2uwG8fMCAwEAAaNaMFgwFgYDVR0RBA8wDYILc2VydmVyX3NpZGUwHQYD
VR0OBBYEFBKms1sOw8nM/O5SN1EZIH+LsWaPMB8GA1UdIwQYMBaAFOGCi133Czav
IQRoqnb5Ew5ZXwPuMA0GCSqGSIb3DQEBCwUAA4IBAQA6H/sfm8YUn86+GwxNR9i9
MCL7WHVRx3gS9ENK87+HtZNL2TVvhPJtupG3Pjgqi33FOHrM4rMUcWSZeCEycVgy
5cjimQLwfDljIBRQE6sem3gKf0obdWl5AlPDLTL/iKj5Su7NycrjZFYqkjZjn+58
fe8lzHNeP/3RQTgjJ98lQI0bdzGDG1+QoxTgPEc77vgN0P4MHJYx2auz/7jYBqNJ
ko8nugIQsd4kOhmOIBUQ8aXkXFktSQIerEGB8uw5iF2cCdH/sTCvhzhxLb4IWo/O
0cAZ+Vs4FW3KUn/Y44yrVAWl1H6xdFsNXBqbzVEMzlt/RV3rH70RDCc20XhP+w+g
-----END CERTIFICATE-----";

        let server_side_key = b"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCsOHis7U9Yp87h
CDwWj5LFso6A/Z+ricfYwENsEQeq14gLF1S0UrnM3Rg6W9TMAKqCIuqM8Lcw0cJK
+fL1DWkwrMMWnZQJgeOCRFWCmaLm5R45VY+coK9EeEeRdk/bSFfo6ambih0eHR3v
D4gwollc1r2MDi1Sl0CknmhNmdeN7pFSCfOapjcpCdOugFYYFpyGEgkfXLO1jwXU
1gFty+MjZWrLfYX78XUDQ7OQLVJySnwF/uw8pTwJvFe2I1NZPQLUzT26cpG/V/ex
K7AfPPdTJmWhmadCerZ7rnA5JNHZaMDFb4tmlArGKCheQ4BsbclQHi4Uz+9Bnp2i
3trsBvHzAgMBAAECggEAUjpIS/CmkOLWYRVoczEr197QMYBnCyUm2TO7PU7IRWbR
GtKR6+MPuWPbHIoaCSlMQARhztdj8BhG1zuOKDi1/7qNDzA/rWZp9RmhZlDquamt
i5xxjEwgQuXW7fn6WO2qo5dlFtGT43vtfeYBlY7+cdhJ+iQOub9j6vWDQYHxrF7x
yM8xvNzomHThvLFzWXJV/nGjX5pqPraMmwJUW+MGX0YaEr6tClqsc1Kmxhs3iIUo
1JCqh3FpVu2i/mR9fdcQ0ONT/s1UHzy+1Bhmh3j2Fuk4+ZeLMfxTfFxk5U0BeMQY
sES3qmd+pG5iqPW+AmXy299G89jf5+1Q4J2Km5KOUQKBgQDidifoeknpi9hRHLLD
w/7KMMe8yYg3c3dv5p0iUQQ2pXd1lJIFQ+B2/D+hfOXhnN/iCDap89ll2LoQ2Q9L
38kQXH06HCM2q11RP0BEsZCG0CnluS+JVNnjs/ALi+yc4HSpzKPs3zXIC3dLOUbq
ov5Esa5h/RU6+NO+DH72TWTv6wKBgQDCryPKtOcLp1eqdwIBRoXdPZeUdZdnwT8+
70DnC+YdOjFkqTbaoYE5ePa3ziGOZyTFhJbPgiwEdj9Ez1JSgqLLv5hBc4s6FigK
D7fOnn7Q7+al/kEW7+X5yoSl1bFuPCqGL1xxzxmpDY8Gf3nyZ+QGfWIenbk3nq12
nTgINyWMGQKBgQDSrxBDxXl8EMGH/MYHQRGKs8UvSuMyi3bjoU4w/eSInno75qPO
yC5NJDJin9sSgar8E54fkSCBExdP01DayvC5CwLqDAFqvBTOIKU/A18tPP6tnRKv
lkQ8Bkxdwai47k07J4qeNa9IU/qA/mGOq2MZL6DHwvd8bMA5gFCh/rDYTwKBgAPm
gGASScK5Ao+evMKLyCjLkBrgVD026O542qMGYQDa5pxuq3Or4qvlGYRLM+7ncBwo
8OCNahZYzCGzyaFvjpVobEN7biGmyfyRngwcrsu+0q8mreUov0HG5etwoZJk0DFK
B58cGBaD+AaYTTgnDrF2l52naUuM+Uq0EahQeocZAoGBAMJEGUFyEdm1JATkNhBv
ruDzj07PCjdvq3lUJix2ZlKlabsi5V+oYxMmrUSU8Nkaxy6O+qETNRNWQeWbPQHL
IZx/qrP32PmWX0IVj3pbfKHQSpOKNGzL9xUJ/FIycZWyT3yGf24KBuJwIx7xSrRx
qNsoty1gY/y3n7SN/iMZo8lO
-----END PRIVATE KEY-----";

        let credentials_txt = b"client1name:client1passwd
client2name:client2passwd";

        let certs_dir = Path::new(file_path);
        if !certs_dir.exists() {
            fs::create_dir(certs_dir)?;
        }
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
        for test_file in test_files.iter() {
            let file_path = certs_dir.join(test_file.name);
            let mut file = fs::File::create(&file_path)?;
            file.write_all(test_file.value)?;
        }

        Ok(())
    }

    async fn get_basic_router_config_tls(port: u16) -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec![format!("tls/127.0.0.1:{}", port).parse().unwrap()];
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
                        "server_private_key": "tests/testfiles/serversidekey.pem",
                        "server_certificate": "tests/testfiles/serverside.pem",
                        "root_ca_certificate": "tests/testfiles/ca.pem",
                        "client_auth": true,
                        "server_name_verification": false
                    },
                    },
                }"#,
            )
            .unwrap();
        config
    }
    async fn get_basic_router_config_quic(port: u16) -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec![format!("quic/127.0.0.1:{}", port).parse().unwrap()];
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
                        "server_private_key": "tests/testfiles/serversidekey.pem",
                        "server_certificate": "tests/testfiles/serverside.pem",
                        "root_ca_certificate": "tests/testfiles/ca.pem",
                        "client_auth": true,
                        "server_name_verification": false
                    },
                    },  
                }"#,
            )
            .unwrap();
        config
    }

    async fn get_basic_router_config_usrpswd() -> Config {
        let mut config = config::default();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config.listen.endpoints = vec!["tcp/127.0.0.1:37447".parse().unwrap()];
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .insert_json5(
                "transport",
                r#"{
                    "auth": {
                        usrpwd: {
                            user: "routername",
                            password: "routerpasswd",
                            dictionary_file: "tests/testfiles/credentials.txt",
                        },
                    },
                }"#,
            )
            .unwrap();
        config
    }
    async fn close_router_session(s: Session) {
        println!("Closing router session");
        ztimeout!(s.close()).unwrap();
    }

    async fn get_client_sessions_tls(port: u16) -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client([format!("tls/127.0.0.1:{}", port)
            .parse::<EndPoint>()
            .unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                    }
                }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();
        let mut config = config::client([format!("tls/127.0.0.1:{}", port)
            .parse::<EndPoint>()
            .unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "tls"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                    }
                }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_quic(port: u16) -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client([format!("quic/127.0.0.1:{}", port)
            .parse::<EndPoint>()
            .unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                    }
                }"#,
            )
            .unwrap();
        let s01 = ztimeout!(zenoh::open(config)).unwrap();
        let mut config = config::client([format!("quic/127.0.0.1:{}", port)
            .parse::<EndPoint>()
            .unwrap()]);
        config
            .insert_json5(
                "transport",
                r#"{
                        "link": {
                        "protocols": [
                            "quic"
                        ],
                        "tls": {
                            "root_ca_certificate": "tests/testfiles/ca.pem",
                            "client_private_key": "tests/testfiles/clientsidekey.pem",
                            "client_certificate": "tests/testfiles/clientside.pem",
                            "client_auth": true,
                            "server_name_verification": false
                        }
                    }
                }"#,
            )
            .unwrap();
        let s02 = ztimeout!(zenoh::open(config)).unwrap();
        (s01, s02)
    }

    async fn get_client_sessions_usrpswd() -> (Session, Session) {
        println!("Opening client sessions");
        let mut config = config::client(["tcp/127.0.0.1:37447".parse::<EndPoint>().unwrap()]);
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
        let mut config = config::client(["tcp/127.0.0.1:37447".parse::<EndPoint>().unwrap()]);
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

    async fn close_sessions(s01: Session, s02: Session) {
        println!("Closing client sessions");
        ztimeout!(s01.close()).unwrap();
        ztimeout!(s02.close()).unwrap();
    }

    async fn test_pub_sub_deny_then_allow_tls(port: u16) {
        println!("test_pub_sub_deny_then_allow_tls");

        let mut config_router = get_basic_router_config_tls(port).await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "permission": "allow",
                            "flows": ["ingress","egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions_tls(port).await;
        {
            let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().deserialize::<String>().unwrap();
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
        let mut config_router = get_basic_router_config_tls(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_tls(port).await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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

        let mut config_router = get_basic_router_config_tls(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "permission": "allow",
                            "flows": ["egress","ingress"],
                            "actions": [
                                "get",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls(port).await;
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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

        let mut config_router = get_basic_router_config_tls(port).await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "get",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_tls(port).await;
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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
                            "permission": "allow",
                            "flows": ["ingress","egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
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
                    *temp_value = sample.payload().deserialize::<String>().unwrap();
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
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
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
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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
                            "permission": "allow",
                            "flows": ["egress","ingress"],
                            "actions": [
                                "get",
                                "declare_queryable"],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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
                    "rules":
                    [
                        {
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "get",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "cert_common_names": [
                                "client_side"
                            ]
                        },
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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

    async fn test_pub_sub_deny_then_allow_usrpswd() {
        println!("test_pub_sub_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;

        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "permission": "allow",
                            "flows": ["ingress","egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (sub_session, pub_session) = get_client_sessions_usrpswd().await;
        {
            let publisher = pub_session.declare_publisher(KEY_EXPR).await.unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber = sub_session
                .declare_subscriber(KEY_EXPR)
                .callback(move |sample| {
                    let mut temp_value = zlock!(temp_recv_value);
                    *temp_value = sample.payload().deserialize::<String>().unwrap();
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

    async fn test_pub_sub_allow_then_deny_usrpswd() {
        println!("test_pub_sub_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "put",
                                "declare_subscriber"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();
        let (sub_session, pub_session) = get_client_sessions_usrpswd().await;
        {
            let publisher = ztimeout!(pub_session.declare_publisher(KEY_EXPR)).unwrap();
            let received_value = Arc::new(Mutex::new(String::new()));
            let temp_recv_value = received_value.clone();
            let subscriber =
                ztimeout!(sub_session
                    .declare_subscriber(KEY_EXPR)
                    .callback(move |sample| {
                        let mut temp_value = zlock!(temp_recv_value);
                        *temp_value = sample.payload().deserialize::<String>().unwrap();
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

    async fn test_get_qbl_deny_then_allow_usrpswd() {
        println!("test_get_qbl_deny_then_allow_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "deny",
                    "rules": [
                        {
                            "permission": "allow",
                            "flows": ["egress","ingress"],
                            "actions": [
                                "get",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();

        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd().await;
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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

    async fn test_get_qbl_allow_then_deny_usrpswd() {
        println!("test_get_qbl_allow_then_deny_usrpswd");

        let mut config_router = get_basic_router_config_usrpswd().await;
        config_router
            .insert_json5(
                "access_control",
                r#"{
                    "enabled": true,
                    "default_permission": "allow",
                    "rules": [
                        {
                            "permission": "deny",
                            "flows": ["egress"],
                            "actions": [
                                "get",
                                "declare_queryable"
                            ],
                            "key_exprs": [
                                "test/demo"
                            ],
                            "usernames": [
                                "client1name",
                                "client2name"
                            ]
                        },
                    ]
                }"#,
            )
            .unwrap();
        println!("Opening router session");

        let session = ztimeout!(zenoh::open(config_router)).unwrap();

        let (get_session, qbl_session) = get_client_sessions_usrpswd().await;
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
                        received_value = sample.payload().deserialize::<String>().unwrap();
                        break;
                    }
                    Err(e) => println!(
                        "Error : {}",
                        e.payload()
                            .deserialize::<String>()
                            .unwrap_or_else(|e| format!("{}", e))
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
}
