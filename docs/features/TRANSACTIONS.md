# Fiche de fonctionnalité — Transactions, erreurs, pool et ressources (client)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLIENT-003` |
| Statut | **Partiel synchrone ; async/pool/erreurs structurées manquants** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §6.4](../CAHIER_DES_CHARGES.md#64-transactions-erreurs-et-ressources), §9 sécu |
| Priorité | P0/P1 |

## 1. Résumé
Fournir transactions ACID, gestion d’erreurs typée, pool asynchrone et observabilité pour le client Rust.

## 2. Contrat §6.4
- `client.$transaction(|tx| async { … })` commit si Ok, rollback si Err, panic → rollback
- `interactiveTransactions` avec isolation `ReadCommitted` par défaut, `Serializable` option, timeout, retry sur deadlock
- Erreurs structurées : `RustormError::UniqueConstraintViolation { field, value }`, `ForeignKeyViolation`, `RecordNotFound`, `QueryError`, `ConnectionError`, avec source `sqlState`
- Pool : `deadpool`/`bb8` async, `max_connections`, `connection_timeout`, `idle_timeout`, `createConnection` sans fuite
- Annulation : requête annulable sans poison du pool
- Observabilité : log requête tronqué/expurgé, métriques pool

## 3. État actuel
- `src/postgres.rs` : `Connection { libpq raw }`, `Transaction { conn, committed }`, `begin/commit/rollback`, `transaction(|tx| fn)` sync, garde Drop rollback si non committed, `exec` avec params `$`→`TEXT`
- Nested writes via savepoints
- **Manques** :
  - Synchrone seul, pas de `async`/`tokio`, pas de pool
  - Pas de `interactiveTransactions` avec options isolation/timeout
  - Erreurs peu structurées : `RustormError::QueryFailed/PgError` génériques, pas de mapping `23505`→UniqueViolation avec champ
  - Pas de retry deadlock, pas de `RecordNotFound` distinct
  - Pas d’observabilité pool (puisqu’il n’y a pas de pool)
  - `Connection` libpq bloquant, pas d’annulation

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-TX01 | Panic dans transaction → rollback, pas de commit partiel |
| VAL-TX02 | Erreur unique → `UniqueConstraintViolation` avec contrainte/table/colonnes |
| VAL-TX03 | Timeout → `ConnectionError` sans fuite |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Tx-01 | Deux creates en transaction + second échoue → première annulée |
| AC-Tx-02 | `RecordNotFound` sur `update`/`delete` absent vs unique violation |
| AC-Tx-03 | Charge concurrente pool `max=5` → sans fuite, métriques ok |

## 6. Tests
- Transaction sync : commit/rollback/panic (déjà partiels dans `tests/postgres.rs`)
- À ajouter : isolation, retry deadlock, mapping erreurs, pool async, annulation

## 7. Risques
| Risque | Réduction |
|---|---|
| Sync→async migration cassante | Feature flag `async`, tests en sync conservés |
| Mapping erreurs incomplet | Table `sqlState`→variant exhaustive, testée |

## 8. Livré / restant
- Livré : Transaction sync avec savepoint, Drop rollback
- Restant : async, pool, isolation/timeout, erreurs structurées, retry, annulation, métriques
