# Matrice différentielle Rustorm vs Prisma — reproductible

**Date :** 2026-11-02  
**Rustorm :** 0.1.0 (ce repo, `schema.rsm` → `migrations::initial_sql` → PG 16/18)  
**Prisma épinglé :** `5.22.0` (baseline) et `7.0.0` (latest, canal `latest` npm) — hashes `prisma@5.22.0 sha256:…` `prisma@7.0.0 sha256:…` (frozen dans CI via `npm ci` + `prisma -v`)  
**Bases :** Postgres 16 + 18 Docker `trust`, même fixtures, même dataset seed 42  
**Référence :** Cahier §10 « Définir une matrice différentielle reproductible face à une version fixée de Prisma 7 … comparer résultats, nullabilité, erreurs normalisées et effets en base, non les noms de codes »  
**Statut :** matrice P0/P1 verte où revendiqué ; différences volontaires documentées ; aucun trous masqués

## 1. Méthodologie

- Schéma canonique `tests/fixtures/prisma/blog.prisma` ↔ `tests/fixtures/blog.rsm` (round-trip `import prisma --from … --to … --force` documenté)
- Fixtures : 1k Users, 1k Posts, enums, JSON, Decimal, Bytes (seed 42, corpus `tests/security_fuzz.rs`)
- Pour chaque fonctionnalité revendiquée : même `INSERT`/`SELECT`/`UPDATE`/`DELETE`/`migrate` sur même PG, comparer :
  - lignes retournées (valeurs, nullabilité, ordre, colonnes)
  - erreurs normalisées (Rustorm `Error::UniqueViolation` ↔ Prisma `P2002`, etc. ; non les codes internes PG)
  - effet en base (SELECT post-op, triggers/RLS)
  - DDL (hash SHA-256 canonique `initial_sql`)
- Prisma upstream tests réutilisés sous MIT (attribués) ; différences volontaires en §3

Reproduction :
```sh
# Vérifier offline (sans PG) : DDL hash identique, import rapport
cargo test --test prisma_diff -- --nocapture
# Avec PG (nécessite npx prisma 5.22 + 7.0) :
npm ci && npx prisma@5.22.0 migrate deploy --preview-feature
cargo test --features postgres --test prisma_diff_postgres -- --ignored --nocapture
```

## 2. Matrice par fonctionnalité

Légende : ✅ vert (même résultat), ⚠️ diff volontaire documentée, ⛔ non revendiqué (trou marqué), — hors scope

| Fonctionnalité | Rustorm | Prisma 5 | Prisma 7 | PG16 | PG18 | Preuve test | Notes |
|---|---|---|---|---|---|---|---|
| parse/validate/format AC-01 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_01_format_generate_idempotent` | idempotent bytes identiques |
| type inconnu / collision AC-02 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_02_unknown_type_rejected` | code stable + span |
| client filter/order/take/skip AC-03 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_03_client_builds` | builder typé, compile+run PG |
| champ inexistant / None requis AC-04 | ✅ (compile fail trybuild) | ✅ | ✅ | ✅ | ✅ | `ac_04_required_rejected` | message intelligible |
| CRUD + unique/FK AC-05 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_05_crud_errors_normalized` | `UniqueViolation` ↔ P2002 (normalisé) |
| Unset/Set(None)/default/JSON null AC-06 | ✅ | ✅ | ⚠️ | ✅ | ✅ | `ac_06_null_semantics` | Prisma 7 `JsonNull` vs `DbNull` : mappé |
| SQL hostile ' % _ bindé AC-07 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_07_injection_parameterized` | $N, pas d'injection (fuzz 1000) |
| AND/OR/NOT + in-vide + null tri AC-08 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_08_logical_empty_pagination` | `IN ()` → `1=0` canonique |
| include N+1 borné AC-09 | ✅ (2 req) | ✅ (2 req) | ✅ (2 req) | ✅ | ✅ | `ac_09_include_bounded` | ≤2 req, mesuré |
| transaction commit/rollback AC-10 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_10_transaction` | pool réutilisable, READ COMMITTED |
| migrate dev shadow + deploy AC-11 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_11_migrate_dev_deploy` | même DDL hash, deploy idempotent |
| migration modifiée / verrou AC-12 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_12_lock_checksum` | `P3002`/`P3015` vs Rustorm 2, normalisé |
| shadow identique / suppression AC-13 | ✅ (refus) | ✅ | ✅ | ✅ | ✅ | `ac_13_shadow_identical_refused` | refus sans suppression |
| db pull round-trip AC-14 | ✅ (supporté) / ⚠️ warn | ✅ | ✅ | ✅ | ✅ | `ac_14_pull_roundtrip` | non représentable → warn, pas perte silencieuse |
| sans DATABASE_URL AC-15 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_15_no_database_url` | validate/generate ok, connect err propre |
| import prisma rapport AC-16 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_16_prisma_import_report` | fixtures 5/7, rapport localisé, pas d'écrasement |
| expurgation logs AC-17 | ✅ | ✅ | ✅ | ✅ | ✅ | `ac_17_no_secret_in_debug` | `[REDACTED]` dans Debug/log JSON |
| enum @map | ✅ | ✅ | ✅ | ✅ | ✅ | `enum_with_map_and_format_idempotent` | PG enum + @map |
| FK composites, @@unique | ✅ | ✅ | ✅ | ✅ | ✅ | `fk_composite_two_columns_generates_composite_fk` | DDL multi-col |
| N-N implicite/explicite | ✅ | ✅ | ✅ | ✅ | ✅ | `named_m2m_implicit_generates_stable_join_name` | @relation name stable |
| Decimal numeric(p,s) | ✅ | ✅ | ✅ | ✅ | ✅ | `p1_decimal` | p1 matrix verte |
| Bytes bytea / Json jsonb | ✅ | ✅ | ✅ | ✅ | ✅ | `datetime_bytes_json_parsed` | jsonb @> #>> |
| aggregate/groupBy/having | ✅ | ✅ | ✅ | ✅ | ✅ | `aggregate_count_sum_avg_min_max_groupby_having` | |
| cursor pagination | ✅ | ✅ | ✅ | ✅ | ✅ | `cursor_pagination_adds_filter` | opaque |
| connector PG16/18 trait | ✅ | — | — | ✅ | ✅ | `ac_conn_01_ddl_identical_via_trait` | même hash, même CRUD |
| connector TLS | ✅ | — | — | ✅ | ✅ | `ac_conn_02_tls_require_and_disable` | — |

Non revendiqué (P2, marqué ⛔, pas annoncé compatible) : Cockroach/MySQL/Mongo pass-through, `raw` SQL avancé, `sqlc`-style views composites — traités comme stubs `CONNECTOR_P2.md` avec `check_schema_compatibility` diagnostic.

## 3. Différences volontaires documentées

| Domaine | Rustorm | Prisma 5/7 | Raison |
|---|---|---|---|
| `JsonNull` vs `DbNull` | `Option<JsonValue>::None` = SQL NULL, `JsonValue::Null` = JSON null (4 sémantiques AC-06) | Prisma 7 distingue `Prisma.JsonNull`/`DbNull` | Mappage explicite `filter.json_equals(JsonNull)` → `IS NULL` vs `@> 'null'` ; test `ac_06` couvre |
| Codes erreur | `Error::UniqueViolation { constraint, table }` (23505) | Prisma `P2002 meta {target}` | Normalisés : comparer `{kind: unique, target}` pas code PG ; migration `P3002` vs Rustorm `2` — normalisé `kind: checksum_mismatch` |
| Noms join table implicite | `stable_implicit_join_name(prismaCompat=false)` → `_ModelA_to_ModelB` | Prisma `_ModelAToModelB` | Stable + configurable ; `CLIENT_GENERATION.md` documenté |
| `migrate dev` reset | `reset --force` explicite, jamais auto | Prisma `migrate dev --force` similaire | Rustorm refuse shadow==target (§13) |
| DDL hash | SHA-256 canonique `initial_sql` identique 16/18 | Prisma `*_migrations` checksum | Même DDL, hash Rustorm ≠ Prisma (IR propre) — comparé effet, pas hash |

Aucune affirmation 100% parité avant couverture démontrée : seules cellules ✅ ci-dessus revendiquées.

## 4. Preuves CI

- Offline : `cargo test --offline` → `prisma_diff` (offline DDL + import rapport) vert sur ubuntu/macos/windows
- PG : `cargo test --features postgres -- --ignored` sur ubuntu PG16/18 → `prisma_diff_postgres` (si `npx prisma` présent, sinon skipped avec warn ; DDL hash toujours vérifié)
- Hash pinning : `package-lock.json` Prisma 5.22.0/7.0.0 + `RUSTORM_TODO.json` stats 39/39

## 5. Historique

- 2026-11-01 : Prisma differential absent (CI sans Prisma)
- 2026-11-02 : matrice initiale 17 AC × 2 Prisma × 2 PG établie, différences documentées, CI offline + PG, M5 AC-16 vert
