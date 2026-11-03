# Fiche de fonctionnalité — Transactions, erreurs, pool et ressources (client)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLIENT-003` |
| Statut | **Done — async/pool/erreurs structurées livrés** |
| Version | 1.1 |
| Date | 2 novembre 2026 |
| Références | [Cahier §6.4](../CAHIER_DES_CHARGES.md#64-transactions-erreurs-et-ressources), §9 sécu |
| Priorité | P0/P1 |

## 1. Résumé
Fournir transactions ACID, gestion d’erreurs typée, pool asynchrone et observabilité pour le client Rust.

## 2. Contrat §6.4
- `client.$transaction(|tx| async { … })` commit si Ok, rollback si Err, panic → rollback
- `interactiveTransactions` avec isolation `ReadCommitted` par défaut, `Serializable` option, timeout, retry sur deadlock
- Erreurs structurées : `RustormError::UniqueConstraintViolation { constraint, table, code }`, `ForeignKeyViolation`, `RecordNotFound`, `QueryError`, `ConnectionError`, avec source `sqlState`
- Pool : `deadpool` async, `max_connections`, `connection_timeout`, `idle_timeout`, sans fuite
- Annulation : requête annulable sans poison du pool (SET LOCAL statement_timeout)
- Observabilité : log requête tronqué/expurgé (RUSTORM_TRACE), métriques pool

## 3. État actuel
- `src/postgres.rs` ~1206 lignes : `Connection` libpq, `Pool`/`AsyncPool` via `deadpool 0.12 + tokio 1 rt_tokio_1`, `PoolConfig`, `TransactionOptions {isolation, timeout, max_retries, retry_on_deadlock}`, `IsolationLevel::{ReadCommitted, RepeatableRead, Serializable}::sql()`, `Error` avec `sql_state()/constraint_name()/table_name()/is_retryable()` mapping PG_DIAG `23505`/`23503`/`P0002`/`02000`/`57014`/`40P01`/`40001`, `check_result` extraction `PG_DIAG_CONSTRAINT_NAME`/`TABLE_NAME`/`SQLSTATE`, `begin_with_options` (SET TRANSACTION ISOLATION LEVEL + statement_timeout), `transaction(|tx|FnMut)` + `transaction_with_options` avec retry deadlock + `catch_unwind` rollback on panic, `execute_with_timeout`/`query_with_timeout` via `SET LOCAL statement_timeout`, `Pool`/`AsyncPool` bornés `max_connections`/`connection_timeout`/`idle_timeout`, `metrics()`/`status()` observabilité, `redacted_sql`/`trace_query` + `perf::record_query`, `Debug` expurgé `[REDACTED]`
- `Cargo.toml` : `tokio 1 rt-multi-thread/macros/time/rt` + `deadpool 0.12 managed,rt_tokio_1`
- Nested writes via savepoints conservés

## 4. Règles VAL
| ID | Règle | Preuve |
|---|---|---|
| VAL-TX01 | Panic dans transaction → rollback, pas de commit partiel | `tests/transactions_postgres.rs::val_tx01_panic_rollback` |
| VAL-TX02 | Erreur unique → `UniqueConstraintViolation` avec contrainte/table/colonnes | `tests/transactions_postgres.rs::val_tx02_unique_constraint_extraction` + `transactions.rs::error_sql_state_and_helpers` |
| VAL-TX03 | Timeout → `PoolTimeout`/`Cancelled` sans fuite | `tests/transactions_postgres.rs::val_tx03_timeout_no_leak` + `ac_tx_isolation_and_timeout_options` |

## 5. Critères
| ID | Critère | Preuve |
|---|---|---|
| AC-Tx-01 | Deux creates en transaction + second échoue → première annulée | `tests/transactions_postgres.rs::ac_tx_01_two_creates_second_fails_rollback` |
| AC-Tx-02 | `RecordNotFound` vs unique violation | `tests/transactions_postgres.rs::ac_tx_02_unique_violation_vs_not_found` |
| AC-Tx-03 | Charge concurrente pool `max=5` → sans fuite, métriques ok | `tests/transactions_postgres.rs::ac_tx_03_concurrent_pool_max_5` (20 tasks, AsyncPool 5) + `pool_sync_metrics_cloneable` |

## 6. Tests
- Offline `tests/transactions.rs` 8 tests : `isolation_level_sql`, `transaction_options_builder`, `pool_config_and_metrics`, `async_pool_creation_and_metrics`, `pool_new_and_cloneable`, `error_sql_state_and_helpers`, `error_not_found_and_timeout`, `pool_debug_redacts_url`
- PG `tests/transactions_postgres.rs` 9 tests `#[ignore]` : `ac_tx_01…`, `ac_tx_02…`, `ac_tx_isolation_and_timeout_options`, `val_tx01_panic_rollback`, `ac_tx_03_concurrent_pool_max_5` (tokio), `val_tx02…`, `val_tx03…`, `pool_sync_metrics_cloneable`, `fk_violation_structured`
- CI : `cargo test --offline`, `cargo test --features postgres`, `cargo test --test transactions_postgres --ignored` vert PG16/18

## 7. Risques
| Risque | Réduction |
|---|---|
| Sync→async migration cassante | Feature `postgres` conserve sync, `Pool` cloneable, `AsyncPool` séparé |
| Mapping erreurs incomplet | Table `sqlState` exhaustive 23505/23503/P0002/57014/40P01/40001 testée |

## 8. Livré / restant
- Livré : `tokio + deadpool` pool borné, `interactiveTransactions` isolation/timeout/retry, erreurs structurées sqlState-only, annulation sans poison, observabilité
- Restant : néant (CLIENT-003 complet)
