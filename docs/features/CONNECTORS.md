# Fiche de fonctionnalité — Connecteurs PostgreSQL (et abstraction P1)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CONN-001` / `CONN-0011` |
| Statut | **Done — trait `Connector` sync + second runtime `sqlx` + TLS vérifié + matrice PG16/18 formelle** |
| Version | 1.1 |
| Date | 2 novembre 2026 |
| Références | [Cahier §6 annexe connecteurs](../CAHIER_DES_CHARGES.md) ; Périmètre §3 ; `src/connector.rs`, `src/postgres.rs` |
| Priorité | P0 (PG) / P1 (abstraction) |

## 1. Résumé
P0 exige PostgreSQL 16 et 18 via connecteur sync testé ; P1 ajoute abstraction multi-connecteur et second runtime async `sqlx`. TLS `verify-full` par défaut, `disable` uniquement local.

## 2. Contrat §6/§3
- P0 : PostgreSQL 16/18, `postgresql` provider, `env("DATABASE_URL")` seule forme d’URL, TLS, pool borné
- IR unique, connecteur `postgres` avec `Connection`/`Transaction`, paramétré, DDL via `libpq` `$1`
- P1 : trait `Connector { query/execute/begin }` avec 2 impls (`Connection`, `MockConnector`) + second runtime `sqlx` async (`SqlxConnector` pool `sqlx::PgPool`)

## 3. État actuel
- `src/postgres.rs` : `postgres` feature, `libpq` FFI, `Connection::connect` force `sslmode=verify-full`, `connect_local_unencrypted(port,user,db)` seul chemin `disable`, `Pool`/`AsyncPool` deadpool+tokio, `Error` avec sqlState
- `src/connector.rs` : `Provider::{Postgresql}`, `SslMode::{Disable,Allow,Prefer,Require,VerifyCa,VerifyFull}`, `TlsConfig{sslmode,sslrootcert,sslcert,sslkey,verify_hostname}` avec `from_url/from_query_str/to_params/is_production_ready`, `CapabilityMatrix{transactions,json,enums,views,composite, tls,pool,savepoints}`, `PgVersionMatrix{16,18 tested}`, `MockConnector` second impl sync, `SqlxConnector` async sous `sqlx` feature (`runtime-tokio,tls-rustls,postgres`) avec `map_sqlx_error` 23505/23503 → `UniqueViolation`
- `src/lib.rs` expose `pub mod connector`
- `Cargo.toml` : `tokio` + `deadpool rt_tokio_1` + `sqlx {runtime-tokio,tls-rustls,postgres}` optional, feature `sqlx = ["postgres", "dep:sqlx"]`
- CI `.github/workflows/ci.yml` matrice `postgres: [16,18]` × 13 jobs (offline + with-connector + 9 PG suites + transactions + shadow + connector)

## 4. Matrice formelle PG16/18 (AC-Conn-01)

| Version | Image CI | Testé | DDL identique | CRUD identique | Tx identique | Notes |
|---|---|---|---|---|---|---|
| 16 | `postgres:16` | ✅ | ✅ | ✅ | ✅ | P0 baseline, trust auth |
| 18 | `postgres:18` | ✅ | ✅ | ✅ | ✅ | P0 latest, même DDL/CRUD/Tx que 16 |

Vérification : même `schema.rsm` génère même `migration.sql` via `rustorm::schema::generate` (hash identique), même `INSERT/SELECT/UPDATE/DELETE` via `Connection` et `SqlxConnector` retournent mêmes `Row` (test `connector_postgres::ac_conn_01_ddl_crud_identical`). `PgVersionMatrix::documented().is_green() && ac_conn_01_verified()`.

## 5. TLS (AC-Conn-02)

- Production `TlsConfig::verify_full()` → `sslmode=verify-full` forcé dans `Connection::connect` (libpq) et `SqlxConnector::connect` (sqlx url doit contenir `sslmode=verify-full`, sinon rejet)
- Local `TlsConfig::disable_for_local()` → seul `connect_local_unencrypted` autorise `disable`, usage explicite, mot de passe fixe `rustorm-local-no-secret`, trust auth
- `SslMode::Require` testé : connexion avec `sslmode=require` via `TlsConfig::require()` → `Connection::connect` avec url `sslmode=require` échoue si serveur sans TLS mais ne panique pas, et n'estpas downgradée vers `disable`
- Tests : `connector_tls::*` offline (parse, encrypts/verifies, production_ready) + PG `tls_secure_must_not_honor_disable`, `tls_require_mode_preserved`, `tls_verify_full_needs_ca` (vérifie erreur `Connection`)

## 6. Trait `Connector` (AC-Conn-03)

```rust
pub trait Connector {
    fn query(&mut self, q: &CompiledQuery) -> Result<Vec<Row>, Error>;
    fn execute(&mut self, q: &CompiledQuery) -> Result<u64, Error>;
    fn begin(&mut self) -> Result<Transaction<'_>, Error>;
}
impl Connector for Connection { ... }
impl Connector for MockConnector { ... } // second impl offline, prouve P1
#[cfg(feature="sqlx")] impl AsyncConnector for SqlxConnector { async fn query... }
```

Compilation vérifiée : `tests/connector_trait.rs` crée `Box<dyn Connector>` avec `Connection` et `MockConnector`, `connector_sqlx.rs` compile avec `sqlx` feature (2 impls).

## 7. Critères

| ID | Critère | Preuve |
|---|---|---|
| AC-Conn-01 | Même schéma sur PG16/18 → DDL et CRUD identiques | `PgVersionMatrix`, `connector_postgres::ac_conn_01*`, CI 16+18 vert |
| AC-Conn-02 | TLS `sslmode=require` fonctionne | `connector_tls::tls_require_*` + `postgres::real_postgres...tls_policy` |
| AC-Conn-03 | Trait `Connector` P1 compile avec deux impls | `connector_trait::trait_has_two_impls`, `MockConnector` + `Connection`, `sqlx` second runtime |

## 8. Tests

- Offline `src/connector.rs` 7 tests : `provider_parse, sslmode_parse_and_tls, tls_from_query_str, tls_production_ready, capability_matrix, pg_matrix_green, validate_provider_ok`
- Offline `tests/connector_trait.rs` 12 tests : provider, tls config, capabilities, matrix, mock impl, trait object
- Offline `tests/connector_tls.rs` 10 tests : sslmode encrypts/verifies, TlsConfig parsing, production_ready
- PG `tests/connector_postgres.rs` 8 tests `#[ignore]` : AC-Conn-01 DDL/CRUD, AC-Conn-02 TLS, AC-Conn-03 trait, provider validation
- PG `tests/connector_sqlx.rs` 4 tests `#[ignore] sqlx` : sqlx connect + query + trait parity + TLS

## 9. Livré / restant

- Livré : trait `Connector` sync + `MockConnector` + `SqlxConnector` sqlx async, `Provider`/`SslMode`/`TlsConfig`/`CapabilityMatrix`/`PgVersionMatrix`, TLS `verify-full` forcé + `disable` local uniquement, matrice PG16/18 formelle + CI
- Restant : néant (CONN-0011 complet)
