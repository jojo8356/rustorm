# Fiche de fonctionnalité — Stratégie de tests et critères d’acceptation AC-01…AC-17

| Métadonnée | Valeur |
|---|---|
| Identifiant | `TEST-001` |
| Statut | **Partiel : unitaires + PG ciblés existent, matrice complète manquante** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §10–§11](../CAHIER_DES_CHARGES.md#10-tests-et-critères-dacceptation-par-palier), RELATIONS.md §10–12 |
| Priorité | P0/P1 |

## 1. Résumé
Couvrir chaque palier (§10) par tests déterministes : parse→generate→DDL→CRUD→relations→nested→CLI, avec base jetable et CI.

## 2. Contrat §10–§11
- Jalons M0–M6 : M0 parse/validate/format, M1 DDL/PG, M2 client CRUD, M3 relations/nested R1, M4 migrations/shadow, M5 introspection/CLI avancée, M6 perf/sécu
- AC-01…AC-17 : chaque AC a dépôt PG (init, migrations, données) + oracle (lignes, DDL, erreurs) + nettoyage
- Matrice : versions Prisma 5/7 × PG 16/18 × fonctionnalité annoncée → verte
- CI : unitaires sans PG, PG via service, `generate --check` bloquant, fuzz sans panic
- Tests non flaky : `Barrier`, `serial_test`, timeout, cleanup `DROP`

## 3. État actuel
- Tests unitaires : `src/schema.rs`, `src/query.rs`, `tests/query.rs`, `tests/cardinalities.rs`, `tests/relations.rs`, `tests/named_relations.rs`, `tests/composite_primary.rs`, `tests/cli.rs`, `tests/migrations.rs` (sans PG)
- Tests PG : `tests/postgres.rs`, `tests/cardinalities_postgres.rs`, `tests/named_postgres.rs`, `tests/relations_postgres.rs`, `tests/composite_postgres.rs`, `tests/shadow_postgres.rs`, `tests/nested_create_postgres.rs`, `tests/nested_disconnect_postgres.rs`, `tests/nested_writes_postgres.rs`, `tests/migrations_postgres.rs`, etc.
- Nested offline : `tests/nested_set_ext.rs`, `nested_upsert.rs`, `nested_connect_or_create.rs`, `nested_graph.rs`
- **Manques** :
  - AC-01 (format), AC-04 (imports), AC-08 (migrations dev), AC-10 (introspection), AC-12 (pool), AC-14 (perf) non couverts
  - Matrice PG 18 non vérifiée, Prisma differential absent (aucune instance Prisma en CI)
  - CI réelle : `.github/workflows` existe mais pas testée ici ; pas de `serial_test`, certains tests PG peuvent flaker en parallèle
  - Pas de corpus fuzz persisté
  - Jalons M0–M6 non versionnés (README liste état mais pas calendrier)

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-TST01 | Chaque AC a au moins un test PG avec oracle |
| VAL-TST02 | Test sans PG → jamais de `DATABASE_URL` requis |
| VAL-TST03 | Test PG → base `rustorm_test_*` unique + drop |

## 5. Critères
| ID | Critère |
|---|---|
| AC-T01 | AC-02 CRUD : tous tests verts sur PG16/18 |
| AC-T02 | AC-03 relations : 1-1/1-N/N–N implicite/explicite verts |
| AC-T03 | AC-04 nested : set/upsert/connect_or_create verts (R2) |
| AC-T04 | CI : `cargo test` + `cargo test --features postgres` + `--check` |

## 6. Risques
| Risque | Réduction |
|---|---|
| Flaky PG | `serial_test`, base unique, retry |
| Matrice jamais verte | Documenter trous, ne pas annoncer compatible |

## 7. Livré / restant
- Livré : socle tests unitaires+PG pour parsing, DDL, CRUD, relations, nested R2 partiel, CLI de base, migrations basiques
- Restant : AC-01/04/08/10/12/14, PG18, Prisma differential, CI complète, fuzz corpus, M0–M6
