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

use std::{num::NonZeroUsize, time::Duration};

use secrecy::ExposeSecret;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
    PgPool,
};
use zenoh_config::AclPostgresConf;
use zenoh_result::{bail, zerror, ZResult};
use zenoh_runtime::ZRuntime;

use super::{parse, AclPolicyDocument, PolicyStore};

pub(super) struct PostgresStore {
    sql: String,
    timeout_ms: u64,
    pool: PgPool,
}

impl PostgresStore {
    pub(super) fn new(conf: AclPostgresConf) -> ZResult<Self> {
        let sql = format!(
            "SELECT {}::text FROM {} WHERE {} = $1",
            ident("column", &conf.document_column)?,
            table_name(&conf.table)?,
            ident("column", &conf.identity_column)?,
        );
        let pool_size = NonZeroUsize::new(conf.pool_size)
            .ok_or_else(|| zerror!("Access control postgres pool_size must not be zero"))?;
        let timeout = Duration::from_millis(conf.timeout_ms);
        let options = open_options(&conf)?;
        let max_connections = pool_size.get() as u32;
        // sqlx spawns idle/lifetime maintenance, which needs a Tokio handle.
        let pool = match tokio::runtime::Handle::try_current() {
            Ok(_) => PgPoolOptions::new()
                .max_connections(max_connections)
                .acquire_timeout(timeout)
                .connect_lazy_with(options),
            Err(_) => ZRuntime::Application.block_on(async {
                PgPoolOptions::new()
                    .max_connections(max_connections)
                    .acquire_timeout(timeout)
                    .connect_lazy_with(options)
            }),
        };
        Ok(Self {
            sql,
            timeout_ms: conf.timeout_ms,
            pool,
        })
    }

    async fn fetch_async(&self, identity: &str) -> ZResult<AclPolicyDocument> {
        let timeout = Duration::from_millis(self.timeout_ms);
        let row: Option<Option<String>> = match tokio::time::timeout(
            timeout,
            sqlx::query_scalar(&self.sql)
                .bind(identity)
                .fetch_optional(&self.pool),
        )
        .await
        {
            Ok(Ok(row)) => row,
            Ok(Err(e)) => {
                return Err(zerror!(
                    "Cannot read identity '{}' from Postgres table: {}",
                    identity,
                    e
                )
                .into());
            }
            Err(_) => {
                return Err(
                    zerror!("Timeout reading identity '{}' from Postgres", identity).into(),
                );
            }
        };

        match row {
            Some(Some(value)) => parse(&value),
            Some(None) | None => {
                tracing::debug!(
                    "No access control document for identity '{}' in Postgres",
                    identity
                );
                Ok(AclPolicyDocument::default())
            }
        }
    }
}

impl PolicyStore for PostgresStore {
    /// Reads the document held in Postgres for one identity.
    ///
    /// A missing row yields an empty document, leaving the identity to be enforced with the
    /// rules that apply to everyone. Being unable to read it at all is an error, so that the
    /// caller can refuse the transport.
    fn fetch(&self, identity: &str) -> ZResult<AclPolicyDocument> {
        ZRuntime::Application.block_in_place(self.fetch_async(identity))
    }
}

fn open_options(conf: &AclPostgresConf) -> ZResult<PgConnectOptions> {
    let mut opts: PgConnectOptions = conf
        .url
        .parse()
        .map_err(|e| zerror!("Invalid Postgres url '{}': {}", conf.url, e))?;
    if let Some(password) = &conf.password {
        opts = opts.password(password.expose_secret().as_str());
    }
    if let Some(path) = &conf.root_ca_certificate {
        if matches!(opts.get_ssl_mode(), PgSslMode::Disable) {
            bail!("Postgres root_ca_certificate is set but the url has sslmode=disable");
        }
        let pem = std::fs::read(path)
            .map_err(|e| zerror!("Cannot read Postgres CA '{}': {}", path, e))?;
        if pem.is_empty() {
            bail!("Postgres CA '{}' contains no certificates", path);
        }
        opts = opts.ssl_root_cert(path);
    }
    Ok(opts)
}

fn ident<'a>(kind: &str, ident: &'a str) -> ZResult<&'a str> {
    if ident.is_empty()
        || !ident.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        || !ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        bail!("Postgres {} '{}' must be a plain identifier", kind, ident);
    }
    Ok(ident)
}

fn table_name(table: &str) -> ZResult<String> {
    let mut parts = table.split('.');
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    if parts.next().is_some() {
        bail!(
            "Postgres table '{}' must be a plain identifier (`name` or `schema.name`)",
            table
        );
    }
    match second {
        None => Ok(ident("table", first)?.to_string()),
        Some(name) => Ok(format!(
            "{}.{}",
            ident("table", first)?,
            ident("table", name)?
        )),
    }
}

#[cfg(test)]
mod tests {
    use zenoh_config::{
        AclConfig, AclIdentitySource, AclPolicyStoreBackend, AclPolicyStoreConf, AclPostgresConf,
    };

    use super::super::super::access_control::acl_interceptor_factories;
    use super::*;

    const POSTGRES_DOCUMENT: &str = r#"{
        "rules": [
            {"id": "pg-rule", "key_exprs": ["a/**"], "messages": ["put"], "permission": "allow"}
        ],
        "subjects": [{"id": "pg-subject", "usernames": ["alice"]}],
        "policies": [{"rules": ["pg-rule"], "subjects": ["pg-subject"]}]
    }"#;

    fn conf(table: &str) -> AclPostgresConf {
        AclPostgresConf {
            url: "postgres://127.0.0.1:5432/zenoh".to_string(),
            password: None,
            table: table.to_string(),
            identity_column: "identity".to_string(),
            document_column: "document".to_string(),
            root_ca_certificate: None,
            timeout_ms: 3_000,
            pool_size: 8,
        }
    }

    #[test]
    fn table_name_must_be_a_plain_identifier() {
        assert!(PostgresStore::new(conf("zenoh_acl")).is_ok());
        assert!(PostgresStore::new(conf("public.zenoh_acl")).is_ok());
        assert!(PostgresStore::new(conf("zenoh-acl")).is_err());
        assert!(PostgresStore::new(conf("zenoh_acl;drop")).is_err());
        assert!(PostgresStore::new(conf("")).is_err());
        assert!(PostgresStore::new(conf("a.b.c")).is_err());
    }

    #[test]
    fn column_names_must_be_plain_identifiers() {
        let mut conf = conf("zenoh_acl");
        conf.identity_column = "identity;drop".to_string();
        assert!(PostgresStore::new(conf.clone()).is_err());
        conf.identity_column = "identity".to_string();
        conf.document_column = "acl-json".to_string();
        assert!(PostgresStore::new(conf).is_err());
    }

    #[test]
    fn pool_size_must_not_be_zero() {
        let mut conf = conf("zenoh_acl");
        conf.pool_size = 0;
        assert!(PostgresStore::new(conf).is_err());
    }

    #[test]
    fn a_missing_ca_file_is_rejected() {
        let mut conf = conf("zenoh_acl");
        conf.root_ca_certificate = Some("/no/such/postgres-ca.pem".to_string());
        assert!(PostgresStore::new(conf).is_err());
    }

    #[test]
    fn a_ca_file_cannot_be_paired_with_sslmode_disable() {
        let mut conf = conf("zenoh_acl");
        conf.url = "postgres://127.0.0.1:5432/zenoh?sslmode=disable".to_string();
        conf.root_ca_certificate = Some("/no/such/postgres-ca.pem".to_string());
        assert!(PostgresStore::new(conf).is_err());
    }

    /// Exercises the round trip against a real server, which the other tests deliberately
    /// do not need. Run it with:
    ///
    /// ```sh
    /// docker run --rm -d -p 5432:5432 -e POSTGRES_HOST_AUTH_METHOD=trust postgres:16-alpine
    /// cargo test -p zenoh --features acl_postgres --lib identity_document_is_read -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires a running Postgres server"]
    async fn identity_document_is_read_from_a_running_server() {
        let url = std::env::var("ZENOH_TEST_POSTGRES_URL")
            .unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5432/postgres".to_string());
        let pool = PgPool::connect(&url).await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS zenoh_acl (
                identity TEXT PRIMARY KEY,
                document TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO zenoh_acl (identity, document) VALUES ($1, $2)
             ON CONFLICT (identity) DO UPDATE SET document = EXCLUDED.document",
        )
        .bind("alice")
        .bind(POSTGRES_DOCUMENT)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM zenoh_acl WHERE identity = $1")
            .bind("absent")
            .execute(&pool)
            .await
            .unwrap();

        let mut store_conf = conf("zenoh_acl");
        store_conf.url = url;
        let store = PostgresStore::new(store_conf).unwrap();

        let document = store.fetch("alice").unwrap();
        assert_eq!(document.rules[0].id, "pg-rule");
        assert_eq!(document.subjects[0].id, "pg-subject");

        // An identity the store knows nothing about is left to the rules applying to everyone,
        // rather than being an error.
        let absent = store.fetch("absent").unwrap();
        assert!(absent.rules.is_empty());

        // A server that cannot be reached is an error, so the caller can refuse the
        // identity instead of enforcing rules it failed to read.
        let mut unreachable_conf = conf("zenoh_acl");
        unreachable_conf.url = "postgres://127.0.0.1:1/zenoh".to_string();
        let unreachable = PostgresStore::new(unreachable_conf).unwrap();
        assert!(unreachable.fetch("alice").is_err());
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
                backend: AclPolicyStoreBackend::Postgres(conf("zenoh_acl")),
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
