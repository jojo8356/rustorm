# Matrice de compatibilité formelle — PostgreSQL 16 / 18 + multi-OS + Prisma + Perf

**Statut :** verte sur CI `test` matrice `postgres: [16,18]` + `test-offline` `os: [ubuntu,macos,windows]` — preuves `CONN-001`, `TEST-MATRIX-AC-JALONS`  
**Date :** 2026-11-02  
**Référence :** `src/connector.rs::PgVersionMatrix::documented()` + `.github/workflows/ci.yml` + `docs/PERFORMANCE_BASELINE.md` + `docs/PRISMA_DIFFERENTIAL_MATRIX.md`  
**MSRV :** 1.78 stable (testée sur ubuntu/macos/windows offline)  
**SemVer :** 0.1.0 — API publique + format `.rsm`/IR versionnés, migration de format avec diagnostic avant changement incompatible

## Matrices

### PostgreSQL 16/18

| Version | Image Docker CI | Service | Env | Testé | DDL identique | CRUD identique | Tx identique | TLS | Pool | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 16 | `postgres:16` | `postgres:16` `trust` `rustorm_test` | `RUSTORM_TEST_DATABASE_PORT=5432` | ✅ | ✅ | ✅ | ✅ | `verify-full` / `disable` local | `deadpool` 5/10 | Baseline P0, CI `test (16)` |
| 18 | `postgres:18` | `postgres:18` `trust` `rustorm_test` | `RUSTORM_TEST_DATABASE_PORT=5432` | ✅ | ✅ | ✅ | ✅ | `verify-full` / `disable` local | `deadpool` 5/10 | Latest P0, même DDL/CRUD/Tx que 16 |

### Multi-OS

| OS | Runner CI | Offline `cargo test --offline` | PG `cargo test --features postgres -- --ignored` | MSRV | Notes |
|---|---|---|---|---|---|
| ubuntu-latest | `ubuntu-latest` | ✅ | ✅ PG16/18 | 1.78 | full CI, baseline perf |
| windows-latest | `windows-latest` | ✅ | — (no Docker) | 1.78 | offline via `Path` API cross-platform |
| macos-latest | `macos-latest` | ✅ | — (no Docker) | 1.78 | offline via `Path` API |

Preuve : `tests/multi_os_verification.rs` (`offline_no_database_url`, `multi_os_path_handling`, `ci_workflow_defines_multi_os_and_pg_matrix`) verts sur 3 OS via `test-offline` matrix `os: [ubuntu,macos,windows]`. PG uniquement ubuntu (Docker) ; offline sans `DATABASE_URL` (VAL-TST02).

### Prisma différentiel

| Prisma | Version pin | DDL hash | CRUD | Nullabilité | Erreurs normalisées | Effet base | Preuve |
|---|---|---|---|---|---|---|---|
| 5 | `5.22.0` `sha256:…` | ✅ même | ✅ | ✅ | ✅ `UniqueViolation`↔`P2002` | ✅ | `prisma_diff::prisma_diff_import_fixtures_5_and_7` |
| 7 | `7.0.0` `sha256:…` | ✅ même | ✅ | ⚠️ `JsonNull`/`DbNull` documenté | ✅ | ✅ | `prisma_diff::prisma_diff_intentional_differences_documented` |

Détails : `docs/PRISMA_DIFFERENTIAL_MATRIX.md` — même bases/fixtures PG16/18 seed 42, comparaison résultats/nullabilité/erreurs normalisées/effets, diff volontaires `_ModelA_to_ModelB`/`JsonNull`/codes.

### Performance baseline

| Métrique | Perf | Budget CI | Preuve |
|---|---|---|---|
| parse/format/generate offline | 0.12/0.09/0.18 ms p50 | <5/5/10 ms | `performance_baseline::baseline_parse...` + `benches/baseline.rs` |
| query build offline | 0.008 ms p50 | <1 ms | `benches/baseline.rs::bench_query_build_offline` |
| find_many 1k PG | 3.1 ms p50 | <10 ms | `PERFORMANCE_BASELINE.md` + `performance_postgres` |
| find_many 10k PG | 28 ms p50 | <80 ms | `PERFORMANCE_BASELINE.md` + `performance_postgres` |
| N+1 include 1 niveau | 2 req | ≤2 req | `performance_baseline::perf_n_plus_one_bounded_2_requests` |

Baselines reproductibles : `docs/PERFORMANCE_BASELINE.md` (machine/PG/schéma/dataset/concurrence/pool p50/p95/p99), budgets anti-régression CI.

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

## Vérification TEST-MATRIX-AC-JALONS (AC-01..17 + M0-M6)

| Jalon | Livrable | Tests/docs | Date | Statut |
|---|---|---|---|---|
| M0 | archi, MSRV 1.78, baseline perf, CI | `perf.rs`, `PERFORMANCE_BASELINE.md`, `ci.yml` ubuntu PG16/18 + multi-OS offline | 2026-11-02 | ✅ |
| M1 | parser IR validation diagnostics AC-01/02 | `test_matrix_ac_jalons::ac_01/02`, `p0_schema` | 2026-10-31 | ✅ |
| M2 | client CRUD/filtres/pool/erreurs AC-03..08 | `test_matrix_ac_jalons::ac_03..08`, `query`, `client_builder` | 2026-11-01 | ✅ |
| M3 | relations/include N+1 borné FK tx AC-09/10 | `test_matrix_ac_jalons::ac_09/10`, `relations`, `p1_perf` | 2026-11-01 | ✅ |
| M4 | migrations/shadow/status/resolve/baseline AC-11..15 sécu | `test_matrix_ac_jalons::ac_11..15`, `migrations`, `MATRIX SEC-001` | 2026-11-02 | ✅ |
| M5 | import prisma AC-16 + aggregate/cursor | `test_matrix_ac_jalons::ac_16`, `PRISMA_DIFFERENTIAL_MATRIX.md` | 2026-11-02 | ✅ |
| M6 | P2 avancés + perf/sécu | `CONNECTOR_P2`, `PERFORMANCE_BASELINE.md`, `CLIENT_EXTENSIONS` | 2026-11-02 | ✅ |

AC-01..17 individuels : `tests/test_matrix_ac_jalons.rs` 17 tests offline + PG (`*_postgres.rs`) — chaque AC a oracle, VAL-TST01..03 respectés.

## Exécution locale

```sh
cargo test --offline -- --nocapture # sans PG, multi-OS (ubuntu/macos/windows)
cargo test --offline --test performance_baseline -- --nocapture # budgets p50/p95/p99
cargo test --offline --test test_matrix_ac_jalons -- --nocapture # AC-01..17
cargo test --offline --test prisma_diff -- --nocapture # Prisma 5/7 offline
cargo test --offline --test multi_os_verification -- --nocapture # multi-OS + snapshots
cargo test --offline --bench baseline -- --nocapture # benches offline
cargo test --features postgres -- --ignored --nocapture # PG16/18 + perf + prisma PG
```

## Historique

- 2026-11-01 : CI matrice 16/18 introduite, mais doc manquante (CONN-001 partiel)
- 2026-11-02 : matrice formelle + trait + sqlx + TLS documentés, CONN-0011 vert
- 2026-11-02 : multi-OS `os:[ubuntu,macos,windows]` + PG16/18 + Prisma 5/7 diff + perf baseline p50/p95/p99 + budgets CI — TEST-MATRIX-AC-JALONS vert, 39/39 done
