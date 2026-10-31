# Fiche de fonctionnalité — Connecteurs PostgreSQL (et abstraction P1)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CONN-001` |
| Statut | **Un connecteur sync libpq livré ; abstraction et second runtime manquants** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §6 annexe connecteurs](../CAHIER_DES_CHARGES.md) ; Périmètre §3 |
| Priorité | P0 (PG) / P1 (abstraction) |

## 1. Résumé
Le P0 exige PostgreSQL 16 et 18 via un connecteur sync testé ; P1 ajoutera abstraction multi-connecteur et éventuellement second runtime (async sqlx).

## 2. Contrat §6/§3
- P0 : PostgreSQL 16/18, `postgresql` provider, `env("DATABASE_URL")` seule forme d’URL, TLS, pool/pooling selon runtime
- IR unique, connecteur unique `postgres` avec `Connection`/`Transaction`, paramétré, DDL via `libpq`
- P1 : trait `Connector` avec `query/exec/transaction`, `sqlx` async optionnel

## 3. État actuel
- `src/postgres.rs` : `postgres` feature, `libpq` FFI, `$N`→`$1` avec texte, `exec`/`query_raw`, `ColumnType::Integer/BigInt/Text/Float/JsonBytes`, mapping `Value`
- Tests `tests/postgres.rs`, `query.rs`, `nested_*.rs` sur PG disposable
- **Manques** : pas d’abstraction trait, pas de sqlx async, pas de TLS explicite, `provider` non-postgresql correctement rejeté en génération mais pas en exécution, pas de test PG 18 natif (seulement 16 vérifié)

## 4. Critères
| ID | Critère |
|---|---|
| AC-Conn-01 | Même schéma sur PG16 et PG18 → DDL et CRUD identiques |
| AC-Conn-02 | TLS `sslmode=require` fonctionne |
| AC-Conn-03 | Trait `Connector` P1 compile avec deux impls |

## 5. Livré / restant
- Livré : connecteur sync libpq pour PG
- Restant : abstraction trait, async sqlx, matrice 16/18 formelle
