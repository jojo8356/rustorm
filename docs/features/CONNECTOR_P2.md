# Fiche de fonctionnalité — Connecteurs P2 (CockroachDB, SQL Server, MongoDB)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CONNECTOR-P2` |
| Statut | **P2** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | Cahier §3 P2, §4 |
| Priorité | P2 |

## 1. Résumé
Ajouter capacités et stubs pour CockroachDB (compat PG), SQL Server, MongoDB, avec feature flags et diagnostic clair.

## 2. Contrat
- Features Cargo : `cockroach`, `mssql`, `mongodb` (désactivés par défaut)
- Trait `Connector` étendu : `capabilities() -> CapabilityMatrix { transactions, json, enums, views }`
- CockroachDB : réutilise postgres + ajustements (pas de `BIGSERIAL`, `CASCADE` diff)
- SQL Server / MongoDB : renvoient `Error::UnsupportedConnector` avec lien doc
- CLI `rustorm validate --connector cockroach` vérifie compatibilité schéma

## 3. Critères
- `cargo check --features cockroach` OK, `--features mssql` diagnostic unsupported
- Matrice capacités documentée

## 4. Tests
- `connector_capabilities`, `cockroach_sql_generation`, `unsupported_connector_error`
