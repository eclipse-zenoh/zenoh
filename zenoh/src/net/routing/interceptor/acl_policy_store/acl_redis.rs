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

use std::{sync::Mutex, time::Duration};

use redis::{aio::MultiplexedConnection, ConnectionAddr, IntoConnectionInfo, TlsCertificates};
use secrecy::ExposeSecret;
use zenoh_config::AclRedisConf;
use zenoh_result::{bail, zerror, ZResult};
use zenoh_runtime::ZRuntime;

use super::{parse, AclPolicyDocument, PolicyStore};

pub(super) struct RedisStore {
    url: String,
    key_prefix: String,
    timeout_ms: u64,
    client: redis::Client,
    /// Shared multiplexed connection. Dropped when a command fails so the next read
    /// opens a new one.
    connection: Mutex<Option<MultiplexedConnection>>,
}

impl RedisStore {
    pub(super) fn new(conf: AclRedisConf) -> ZResult<Self> {
        let client = open_client(&conf)?;
        Ok(Self {
            url: conf.url,
            key_prefix: conf.key_prefix,
            timeout_ms: conf.timeout_ms,
            client,
            connection: Mutex::new(None),
        })
    }

    async fn multiplexed_connection(&self) -> ZResult<MultiplexedConnection> {
        if let Some(connection) = self.connection.lock().unwrap().clone() {
            return Ok(connection);
        }
        let timeout = Duration::from_millis(self.timeout_ms);
        let connection =
            tokio::time::timeout(timeout, self.client.get_multiplexed_async_connection())
                .await
                .map_err(|_| zerror!("Timeout connecting to Redis at '{}'", self.url))?
                .map_err(|e| zerror!("Cannot connect to Redis at '{}': {}", self.url, e))?;
        *self.connection.lock().unwrap() = Some(connection.clone());
        Ok(connection)
    }

    async fn fetch_async(&self, identity: &str) -> ZResult<AclPolicyDocument> {
        let timeout = Duration::from_millis(self.timeout_ms);
        let key = format!("{}{}", self.key_prefix, identity);
        let mut connection = self.multiplexed_connection().await?;

        let value: Option<String> = match tokio::time::timeout(
            timeout,
            redis::cmd("GET")
                .arg(&key)
                .query_async::<Option<String>>(&mut connection),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(e)) => {
                self.connection.lock().unwrap().take();
                return Err(zerror!("Cannot read key '{}' from Redis: {}", key, e).into());
            }
            Err(_) => {
                self.connection.lock().unwrap().take();
                return Err(zerror!("Timeout reading key '{}' from Redis", key).into());
            }
        };

        match value {
            Some(value) => parse(&value),
            None => {
                tracing::debug!("No access control document at key '{}'", key);
                Ok(AclPolicyDocument::default())
            }
        }
    }
}

impl PolicyStore for RedisStore {
    /// Reads the document held in Redis for one identity.
    ///
    /// An absent key yields an empty document, leaving the identity to be enforced with the
    /// rules that apply to everyone. Being unable to read it at all is an error, so that the
    /// caller can refuse the transport.
    fn fetch(&self, identity: &str) -> ZResult<AclPolicyDocument> {
        ZRuntime::Application.block_in_place(self.fetch_async(identity))
    }
}

fn open_client(conf: &AclRedisConf) -> ZResult<redis::Client> {
    let mut connection_info = conf
        .url
        .as_str()
        .into_connection_info()
        .map_err(|e| zerror!("Invalid Redis url '{}': {}", conf.url, e))?;
    if let Some(password) = &conf.password {
        connection_info.redis.password = Some(password.expose_secret().to_string());
    }
    let client = match &conf.root_ca_certificate {
        None => redis::Client::open(connection_info),
        Some(path) => {
            if !matches!(connection_info.addr, ConnectionAddr::TcpTls { .. }) {
                bail!("Redis root_ca_certificate is set but the url is not rediss://");
            }
            let pem = std::fs::read(path)
                .map_err(|e| zerror!("Cannot read Redis CA '{}': {}", path, e))?;
            if pem.is_empty() {
                bail!("Redis CA '{}' contains no certificates", path);
            }
            redis::Client::build_with_tls(
                connection_info,
                TlsCertificates {
                    client_tls: None,
                    root_cert: Some(pem),
                },
            )
        }
    };
    client.map_err(|e| zerror!("Invalid Redis url '{}': {}", conf.url, e).into())
}

#[cfg(test)]
mod tests {
    use zenoh_config::{
        AclConfig, AclIdentitySource, AclPolicyStoreBackend, AclPolicyStoreConf, AclRedisConf,
    };

    use super::super::super::access_control::acl_interceptor_factories;
    use super::*;

    const REDIS_DOCUMENT: &str = r#"{
        "rules": [
            {"id": "redis-rule", "key_exprs": ["a/**"], "messages": ["put"], "permission": "allow"}
        ],
        "subjects": [{"id": "redis-subject", "usernames": ["alice"]}],
        "policies": [{"rules": ["redis-rule"], "subjects": ["redis-subject"]}]
    }"#;

    /// Exercises the round trip against a real server, which the other tests deliberately
    /// do not need. Run it with:
    ///
    /// ```sh
    /// docker run --rm -d -p 6379:6379 redis:7-alpine
    /// cargo test -p zenoh --features acl_redis --lib identity_document_is_read -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires a running Redis server"]
    async fn identity_document_is_read_from_a_running_server() {
        let url = std::env::var("ZENOH_TEST_REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let client = redis::Client::open(url.as_str()).unwrap();
        let mut connection = client.get_multiplexed_async_connection().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg("zenoh:acl:alice")
            .arg(REDIS_DOCUMENT)
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: () = redis::cmd("DEL")
            .arg("zenoh:acl:absent")
            .query_async(&mut connection)
            .await
            .unwrap();

        let conf = redis_conf(&url);
        let store = RedisStore::new(conf).unwrap();

        let document = store.fetch("alice").unwrap();
        assert_eq!(document.rules[0].id, "redis-rule");
        assert_eq!(document.subjects[0].id, "redis-subject");

        // An identity the store knows nothing about is left to the rules applying to everyone,
        // rather than being an error.
        let absent = store.fetch("absent").unwrap();
        assert!(absent.rules.is_empty());

        // A server that cannot be reached is an error, so the caller can refuse the
        // identity instead of enforcing rules it failed to read.
        let unreachable = RedisStore::new(redis_conf("redis://127.0.0.1:1")).unwrap();
        assert!(unreachable.fetch("alice").is_err());
    }

    fn redis_conf(url: &str) -> AclRedisConf {
        AclRedisConf {
            url: url.to_string(),
            password: None,
            key_prefix: "zenoh:acl:".to_string(),
            root_ca_certificate: None,
            timeout_ms: 3_000,
        }
    }

    #[test]
    fn a_missing_ca_file_is_rejected() {
        let mut conf = redis_conf("rediss://127.0.0.1:6379");
        conf.root_ca_certificate = Some("/no/such/redis-ca.pem".to_string());
        assert!(RedisStore::new(conf).is_err());
    }

    #[test]
    fn a_ca_file_cannot_be_paired_with_a_plaintext_url() {
        let mut conf = redis_conf("redis://127.0.0.1:6379");
        conf.root_ca_certificate = Some("/no/such/redis-ca.pem".to_string());
        assert!(RedisStore::new(conf).is_err());
    }

    #[test]
    fn a_policy_store_needs_no_rules_in_the_configuration_file() {
        // A deployment holding every rule in the store declares none here, which
        // `PolicyEnforcer::init` refuses outright unless the lists are filled in.
        let config = AclConfig {
            enabled: true,
            policy_store: Some(AclPolicyStoreConf {
                identity: AclIdentitySource::Username,
                entry_ttl_ms: None,
                cache_capacity: 8,
                backend: AclPolicyStoreBackend::Redis(redis_conf("redis://127.0.0.1:6379")),
            }),
            ..AclConfig::default()
        };
        assert!(config.rules.is_none());

        let Some((_, state)) = acl_interceptor_factories(&config).unwrap() else {
            panic!("expected an access control factory");
        };
        // The policies are reachable from the routing tables, so that a refresh can read
        // them again without going through a factory.
        assert!(state.is_some());
    }
}
