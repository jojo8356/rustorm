# Matrice de compatibilité formelle — PostgreSQL 16 / 18

**Statut :** verte sur CI `test` matrice `postgres: [16,18]` — preuve `CONN-001` AC-Conn-01
**Date :** 2026-11-02
**Référence :** `src/connector.rs::PgVersionMatrix::documented()` + `.github/workflows/ci.yml`

| Version | Image Docker CI | Service | Env | Testé | DDL identique | CRUD identique | Tx identique | TLS | Pool | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 16 | `postgres:16` | `postgres:16` `trust` `rustorm_test` | `RUSTORM_TEST_DATABASE_PORT=5432` | ✅ | ✅ | ✅ | ✅ | `verify-full` / `disable` local | `deadpool` 5/10 | Baseline P0, CI `test (16)` |
| 18 | `postgres:18` | `postgres:18` `trust` `rustorm_test` | `RUSTORM_TEST_DATABASE_PORT=5432` | ✅ | ✅ | ✅ | ✅ | `verify-full` / `disable` local | `deadpool` 5/10 | Latest P0, même DDL/CRUD/Tx que 16 |

## Vérification AC-Conn-01

- **Définition :** même `schema.rsm` → même `migration.sql` (hash SHA-256 canonique identique) sur 16 et 18.
- **Test :** `connector_postgres::ac_conn_01_ddl_identical_via_trait` compare `migrations::initial_sql(&schema)` sur même schéma, puis exécute `CREATE TABLE`, `INSERT`, `SELECT`, `UPDATE`, `DELETE` via `Box<dyn Connector>` sur chaque version — résultat identique.
- **Preuve CI :** jobs `test (16)` et `test (18)` verts, `PgVersionMatrix::ac_conn_01_verified() == true`.

## Vérification AC-Conn-02 (TLS)

- `TlsConfig::verify_full()` → `sslmode=verify-full` forcé en production (`Connection::connect` + `SqlxConnector::connect`)
- `TlsConfig::disable_for_local()` → seul `connect_local_unencrypted` autorise `disable`, mdp fixe `rustorm-local-no-secret`, trust auth
- `TlsConfig::require()` → `sslmode=require` préservé, test `connector_postgres::ac_conn_02_tls_require_and_disable`
- `secure connector must not honor sslmode=disable` : `Connection::connect("... sslmode=disable")` → `Err(Connection)` (test `postgres::real_postgres_crud...`)

## Vérification AC-Conn-03 (trait)

- `src/postgres.rs::trait Connector { query, execute, begin }`
- `src/connector.rs::mock::MockConnector` impl `Connector` (offline)
- `src/connector.rs::sqlx_impl::SqlxConnector` impl `AsyncConnector` + `Connector` via trait object (feature `sqlx`)
- Tests `connector_trait::trait_has_two_impls_mock_and_connection` + `trait_object_dyn_connector` — compilation avec deux impls vérifiée.

## Vérification SEC-001 (sécurité, isolation, RLS)

| Critère | PG16 | PG18 | Test offline | Test PG |
|---|---|---|---|---|
| `DEFERRABLE`/`SET DEFAULT` rejeté au parse | ✅ | ✅ | `security_fuzz::fuzz_schema_rejects_deferrable_set_default` | — |
| `$N` paramétré, injection OWASP traitée comme valeur | ✅ | ✅ | `security_fuzz::fuzz_injection_payloads_are_parameterized`, `fuzz_all_query_builders_are_parameterized` | — |
| `READ COMMITTED` défaut, `SERIALIZABLE` bloqué | ✅ | ✅ | — | `security_postgres::isolation_must_be_read_committed`, `transaction_uses_read_committed_default` |
| RLS `row_security_active` bloquant | ✅ | ✅ | — | `security_postgres::rls_blocks_nested_write` (FORCE RLS) |
| Trigger `pg_trigger` bloquant | ✅ | ✅ | — | `security_postgres::trigger_blocks_nested_write` |
| Rule `pg_rewrite` bloquant | ✅ | ✅ | — | `security_postgres::rule_blocks_nested_write` |
| FK `NOT deferrable` + `ON UPDATE CASCADE` testé | ✅ | ✅ | — | `security_postgres::on_update_cascade_is_tested` |
| Expurgation `redacted_url` `[REDACTED]` | ✅ | ✅ | `security_fuzz::expurgation_*` | — |
| Shadow éphémère `ShadowTxGuard` rollback même en erreur | ✅ | ✅ | — | `security_postgres::shadow_ephemeral_even_on_error` |

- **Fuzz** : 1000 entrées aléatoires `fuzz_schema_no_panic_on_random` sans panic, online PG identique.
- **Isolation documentée** : `SECURITY.md` §7 détaille `READ COMMITTED`, RLS, trigger/rule, deferrable.

## Exécution locale

```sh
cargo test --offline -- --nocapture # sans PG, 30+ offline + security_fuzz 6
cargo test --features postgres -- --ignored --nocapture # PG16/18 + security_postgres 7
cargo test --features postgres --test connector_postgres -- --ignored --nocapture # PG16/18 trait
cargo test --features sqlx --test connector_sqlx -- --ignored --nocapture # second runtime
```

## Historique

- 2026-11-01 : CI matrice 16/18 introduite, mais doc manquante (CONN-001 partiel)
- 2026-11-02 : matrice formelle + trait + sqlx + TLS documentés, CONN-0011 vert
