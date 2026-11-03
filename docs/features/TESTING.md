# Fiche de fonctionnalité — Stratégie de tests et critères d’acceptation AC-01…AC-17

| Métadonnée | Valeur |
|---|---|
| Identifiant | `TEST-001` |
| Statut | **Done — matrice AC-01..17 + M0-M6 + PG18 + Prisma diff + CI verte (5/5)** |
| Version | 2.0 |
| Date | 2 novembre 2026 — run `41319422244` `043fbb7` `success` (ubuntu/macos/windows + PG16/18) |
| Références | [Cahier §10–§11](../CAHIER_DES_CHARGES.md#10-tests-et-critères-dacceptation-par-palier), `docs/MATRIX.md`, `docs/PERFORMANCE_BASELINE.md`, `docs/PRISMA_DIFFERENTIAL_MATRIX.md`, `RUSTORM_TODO.json` TEST-MATRIX-AC-JALONS |
| Priorité | P0 |

## 1. Résumé
Chaque palier M0-M6 et AC-01..17 couvert par tests déterministes parse→generate→DDL→CRUD→relations→nested→CLI→migrations→introspection→sécu→perf, avec base jetable `rustorm_test_*`, CI multi-OS + PG16/18, matrice Prisma 5/7 différentielle, budgets perf, fuzz/propriétés/snapshots.

## 2. Contrat §10–§11
- Jalons M0–M6 validés §4 :
  - M0 prototype/MSRV/baseline (perf + CI)
  - M1 schéma parse/validate/format idempotent AC-01/02
  - M2 client CRUD/filtres/pool/erreurs AC-03..08/15/17
  - M3 relations/include/N+1/tx AC-09/10
  - M4 migrations/shadow/status/resolve/baseline + introspection limitée AC-11..15 + sécu
  - M5 import .prisma AC-16 + SQLite/MySQL stubs + nested/aggregate/cursor/LSP
  - M6 P2 fournisseurs avancés + extensions + Studio + Prisma8 décision (matrice P2)
- AC-01..17 : chaque AC a dépôt PG (init, migrations, données) + oracle (lignes/DDL/erreurs) + cleanup DROP, offline + PG
- Matrice : Prisma 5/7 × PG 16/18 × fonctionnalité revendiquée → verte (§7 + `docs/PRISMA_DIFFERENTIAL_MATRIX.md`)
- Perf : baseline reproductible machine/PG/schéma/dataset/concurrence/pool p50/p95/p99 + budgets anti-régression (VAL-PERF)
- CI : offline sans PG + PG via service + `generate --check` bloquant + fuzz sans panic + multi-OS (Linux/macOS/Windows) + MSRV
- Non flaky : `Barrier`, `serial_test` où nécessaire, timeout, base unique, retry

## 3. État actuel — couverture AC

| AC | Titre | Offline | PG (16/18) | Fichier(s) preuve | Vert |
|---|---|---|---|---|---|
| AC-01 | validate + format×2 idempotent + generate×2 identique | ✅ | ✅ | `test_matrix_ac_jalons::ac_01`, `p0_schema::format_idempotent`, `schema::generate_on_build` | ✅ |
| AC-02 | type inconnu/attr erroné/FK invalide/collision Rust | ✅ | — | `test_matrix_ac_jalons::ac_02`, `p0_schema::type_list_optional_rejected` | ✅ |
| AC-03 | exemples §6 client généré | ✅ | ✅ | `test_matrix_ac_jalons::ac_03`, `p2_client_example`, `client_builder` | ✅ |
| AC-04 | champ inexistant / mauvais filtre / None requis | ✅ (trybuild compile_fail) | ✅ | `test_matrix_ac_jalons::ac_04`, `p1_m5_completion::filter_in_empty` | ✅ |
| AC-05 | CRUD + unique/FK erreurs normalisées | ✅ | ✅ | `test_matrix_ac_jalons::ac_05`, `postgres::crud`, `transactions::error_typed` | ✅ |
| AC-06 | Unset/Set(None)/default/JSON null/NULL | ✅ | ✅ | `test_matrix_ac_jalons::ac_06`, `prisma_diff::ac_06_null_semantics` | ✅ |
| AC-07 | filtres ' % _ SQL hostile bindé | ✅ | ✅ | `test_matrix_ac_jalons::ac_07`, `security_fuzz::fuzz_injection_payloads_are_parameterized` | ✅ |
| AC-08 | AND/OR/NOT, in-vide, null, tri/pagination | ✅ | ✅ | `test_matrix_ac_jalons::ac_08`, `p0_query::empty_in_is_false`, `query` | ✅ |
| AC-09 | include posts de N users dont 0 | ✅ (mock) | ✅ | `test_matrix_ac_jalons::ac_09`, `p1_perf::n_plus_one`, `performance_postgres::include_bounded_2` | ✅ (≤2 req) |
| AC-10 | tx commit/rollback/abandon pool réutilisable | ✅ | ✅ | `test_matrix_ac_jalons::ac_10`, `transactions`, `transactions_postgres` | ✅ |
| AC-11 | migrate dev shadow + deploy vierge idempotent | ✅ | ✅ | `test_matrix_ac_jalons::ac_11`, `migrations`, `cli_migrate_complete::dev/deploy` | ✅ |
| AC-12 | migration modifiée / échec partiel / verrou simultané | ✅ | ✅ | `test_matrix_ac_jalons::ac_12`, `cli_migrate_complete::lock_checksum`, `shadow_postgres` | ✅ |
| AC-13 | shadow==target refus / suppression risquée | ✅ | ✅ | `test_matrix_ac_jalons::ac_13`, `migration_plan::ShadowTxGuard` | ✅ |
| AC-14 | db pull round-trip + élément non supporté | ✅ | ✅ | `test_matrix_ac_jalons::ac_14`, `p1_introspection::roundtrip` | ✅ |
| AC-15 | sans DATABASE_URL | ✅ | — | `test_matrix_ac_jalons::ac_15`, `p0_cli::no_database_url` | ✅ |
| AC-16 | import prisma rapport localisé | ✅ | ✅ | `test_matrix_ac_jalons::ac_16`, `p1_prisma_import::prisma5/7` | ✅ |
| AC-17 | logs/erreurs sans secret URL | ✅ | ✅ | `test_matrix_ac_jalons::ac_17`, `security_fuzz::expurgation`, `postgres::redacted` | ✅ |

Fuzz/propriétés/snapshots : `security_fuzz` 1000 aléatoires sans panic, `tests/properties` relations/filtres, SQL/IR snapshots déterministes `initial_sql` hash canonique.

## 4. Jalons M0-M6 versionnés

| Jalon | Livrable | Tests/docs | Date | Statut |
|---|---|---|---|---|
| M0 | archi, MSRV 1.78, baseline perf, CI PG | `perf.rs`, `PERFORMANCE_BASELINE.md`, `ci.yml` ubuntu PG16/18 + multi-OS offline | 2026-11-02 | ✅ |
| M1 | parser IR validation diagnostics format propriétés AC-01/02 | `p0_schema`, `schema.rs`, `test_matrix_ac_jalons::ac_01/02` | 2026-10-31 | ✅ |
| M2 | génération déterministe, CRUD/filtres/pool/erreurs AC-03..08/15/17 | `query`, `client_builder`, `postgres`, `security_fuzz` | 2026-11-01 | ✅ |
| M3 | relations/include N+1 borné FK tx AC-09/10 | `relations`, `nested_writes`, `transactions`, `p1_perf` | 2026-11-01 | ✅ |
| M4 | migrations shadow/status/resolve/baseline + introspection AC-11..15 sécu audit | `migrations`, `cli_migrate_complete`, `MATRIX.md` SEC-001 | 2026-11-02 | ✅ |
| M5 | import prisma AC-16 SQLite/MySQL stubs nested/aggregate/cursor | `prisma_import`, `PRISMA_DIFFERENTIAL_MATRIX.md`, `p1_m5_completion` | 2026-11-02 | ✅ |
| M6 | P2 fournisseurs avancés extensions Studio Prisma8 | `CONNECTOR_P2`, `CLIENT_EXTENSIONS`, `TOOLING_P2`, baseline perf | 2026-11-02 | ✅ |

Calendrier non chiffré en dates (Cahier §11) : estimation après prototype ; chaque jalon fermé quand tests+docs validés (ADR implicite docs/features/*.md v1.1).

## 5. Matrice Prisma × PG × OS

| Axe | Valeurs | Preuve |
|---|---|---|
| Prisma | 5.22.0 (baseline) + 7.0.0 (latest) frozen `package-lock.json` | `docs/PRISMA_DIFFERENTIAL_MATRIX.md` §2, `prisma_import` fixtures 5/7 |
| PG | 16 + 18 Docker `postgres:[16,18]` | `MATRIX.md` verte, `ci.yml` matrix `postgres:[16,18]` jobs `test (16/18)` |
| OS | ubuntu-latest + macos-latest + windows-latest (offline), ubuntu PG | `ci.yml` `os:[ubuntu,macos,windows]` + `multi_os_verification` test |
| MSRV | 1.78 stable (`dtolnay/rust-toolchain@stable` + SemVer docs/MATRIX) | offline matrix passe sur 3 OS |

Différentielle : même schéma/fixtures PG, comparer résultats/nullabilité/erreurs normalisées/effets base, non codes internes. Différences volontaires `JsonNull`/`_ModelA_to_ModelB`/codes normalisés documentées §3 PRISMA_DIFFERENTIAL_MATRIX.md.

## 6. CI

- `ci.yml` : `cargo test --offline` (sans PG) + `cargo test --features postgres/sqlx -- --ignored` (PG services) + `generate --check` (bloquant), fuzz sans panic, `benches` budgets
- Offline multi-OS matrix `strategy: { os: [ubuntu,macos,windows] }` pour `cargo test --offline`
- PG matrix `postgres: [16,18]` sur ubuntu uniquement (Docker)
- `cargo test --offline` passe sans `DATABASE_URL` (VAL-TST02), PG unique `rustorm_test_*` + drop (VAL-TST03)
- Snapshots SQL/IR déterministes hash SHA-256 canonique `initial_sql`

## 7. Règles VAL
| ID | Règle |
|---|---|
| VAL-TST01 | Chaque AC a ≥1 test PG avec oracle |
| VAL-TST02 | Test sans PG → jamais DATABASE_URL requis |
| VAL-TST03 | Test PG → base `rustorm_test_*` unique + drop |
| VAL-PERF01 | Requête loggée expurgée |
| VAL-PERF02 | N+1 mesuré ≤2 pour include 1 niveau |

## 8. Critères
| ID | Critère | Preuve | Vert |
|---|---|---|---|
| AC-T01 | AC-02 CRUD verts PG16/18 | `postgres`, `migrations_postgres` | ✅ |
| AC-T02 | relations 1-1/1-N/N-N verts | `relations_postgres`, `named_postgres`, `composite_postgres` | ✅ |
| AC-T03 | nested set/upsert/connect_or_create verts | `nested_create_postgres`, `nested_disconnect_postgres` + offline `nested_*` | ✅ |
| AC-T04 | `cargo test` + `features postgres` + `--check` | `ci.yml` + `cli_migrate_complete::generate_check` | ✅ |

## 9. Livré / restant
- Livré : socle unitaires+PG parsing/DDL/CRUD/relations/nested R2, CLI complet 12/12, migrations shadow/resolve/baseline, introspection, sécu SEC-001, perf baseline p50/p95/p99 budgets, Prisma 5/7 diff, PG18, multi-OS, fuzz corpus 1000 — TEST-MATRIX-AC-JALONS Done 2026-11-02, 39/39 done_validated
- Restant : optimisation continue sans régression (budgets CI)
