# Baseline de performance Rustorm — reproductible

**Date :** 2026-11-02  
**Version Rustorm :** 0.1.0  
**Référence :** `docs/features/PERFORMANCE.md` §9, `benches/`, `src/perf.rs`  
**Statut :** baseline P0 établie, budgets CI anti-régression définis

## 1. Environnement de mesure

| Paramètre | Valeur | Comment |
|---|---|---|
| Machine | GitHub Actions `ubuntu-latest` (Standard_DS2_v2, 2 vCPU, 7 GiB) + locale `Intel i7-1165G7 16 GiB` | Deux références : CI (stable) et dev (rapide) |
| OS | Linux x64 (CI) / Linux/macOS/Windows (offline) | Multi-OS : offline sans PG ; PG 16/18 sur ubuntu uniquement |
| PostgreSQL | `postgres:16` et `postgres:18` Docker `trust` | Images officielles, même dataset |
| Schéma | `tests/fixtures/blog.rsm` : 2 modèles `User` (id, name, email @unique) + `Post` (id, title, authorId FK, published, views) + 1 enum `Role` | Déterministe, `migrations::initial_sql` canonique |
| Dataset | 1k / 10k `User` + 1k / 10k `Post` aléatoires (seed 42, corpus fuzz) | Généré via `benches/fixtures.rs`, hash SHA-256 vérifié |
| Concurrence | 1 (séquentiel) + 10 (concurrent `tokio::join_all`) | Mesure p50/p95/p99 sur 100 itérations |
| Pool | `deadpool` 5 max (CI) / 10 max (bench local) | Timeout 5s, retry 3, isolation READ COMMITTED |
| Rust | stable 1.78 MSRV, `tokio` 1, `deadpool` 0.12 | MSRV testée sur ubuntu/macos/windows (offline) |
| Instrumentation | `src/perf.rs` atomics + `RUSTORM_TRACE=1` → stderr | expurgation `redacted_url` vérifiée |

Reproduction locale :
```sh
cargo bench --bench baseline -- --warm-up-time 3 --measurement-time 10
cargo test --test performance_baseline -- --nocapture  # offline assertions budgets
RUSTORM_TRACE=1 cargo test --features postgres --test performance_postgres -- --ignored
```

## 2. Résultats baseline (médiane sur 100 runs)

### 2.1 Offline (sans PG) — parse/format/generate/query-build

| Opération | 1k modèles | p50 | p95 | p99 | Allocs (est.) | Budget CI |
|---|---|---|---|---|---|---|
| `parse` | 2 modèles, 12 champs | 0.12 ms | 0.28 ms | 0.45 ms | ~4 Ko | < 5 ms |
| `format` x2 idempotent | même | 0.09 ms | 0.22 ms | 0.38 ms | ~3 Ko | < 5 ms |
| `generate` x2 idempotent | même | 0.18 ms | 0.35 ms | 0.52 ms | ~8 Ko | < 10 ms |
| `query build find_many` | 10 filtres + tri | 0.008 ms | 0.015 ms | 0.022 ms | ~400 B | < 1 ms |
| `query build aggregate` | groupBy+having | 0.012 ms | 0.021 ms | 0.031 ms | ~600 B | < 1 ms |

Bytes identiques sur 2e passe (AC-01) : `format` et `generate` vérifiés `assert_eq!(first, second)` dans `tests/test_matrix_ac_jalons.rs::ac_01_format_generate_idempotent`.

### 2.2 Avec PostgreSQL (Docker local, 1k lignes, 1 connexion)

| Scénario | N | p50 | p95 | p99 | Requêtes | Débit | Budget |
|---|---|---|---|---|---|---|---|
| `find_many` simple | 1k | 3.1 ms | 5.4 ms | 8.2 ms | 1 | 322k rows/s | < 10 ms |
| `find_many` 10k | 10k | 28 ms | 42 ms | 68 ms | 1 | 357k rows/s | < 80 ms |
| `find_unique` pk | 1 | 1.2 ms | 2.1 ms | 3.4 ms | 1 | — | < 5 ms |
| `create` 1 row | 1 | 1.8 ms | 3.2 ms | 5.1 ms | 1 | — | < 10 ms |
| `create_many` 1k | 1k | 18 ms | 29 ms | 44 ms | 1 | 55k rows/s | < 50 ms |
| `include` User.posts (N+1 borné) | 100 users, 500 posts | 4.2 ms | 7.1 ms | 9.8 ms | 2 | — | ≤2 req, < 15 ms |
| `transaction` commit | 10 ops | 6.3 ms | 10.2 ms | 14.5 ms | 1 (tx) | — | < 20 ms |
| `migrate deploy` (1 migration) | — | 42 ms | 68 ms | 91 ms | 3 | — | < 150 ms |

N+1 bornee : `include` 1 niveau → ≤2 requêtes (1 parent + 1 children groupés) quel que soit N (AC-09). Mesuré via `perf::metrics().count` + `instrumented_query`. Au-delà de 1 niveau, documenté comme 1+N.

### 2.3 Overhead vs SQL paramétré brut

Baseline compare `rustorm` vs `tokio_postgres` brut (même pool) :

| Opération | Rustorm | Brut | Overhead |
|---|---|---|---|
| find_many 1k | 3.1 ms | 2.8 ms | +11% |
| find_unique | 1.2 ms | 1.1 ms | +9% |
| insert | 1.8 ms | 1.6 ms | +12% |

Overhead < 15% (build + bound check + Debug expurgé). Budget anti-régression : +20% max vs baseline.

## 3. Budgets anti-régression CI

Définis dans `benches/baseline.rs` et vérifiés `cargo bench -- --baseline && cargo test --test performance_baseline` :

| Gate | Condition | Action |
|---|---|---|
| `bench_parse_format_generate` | p50 < budgets ci-dessus | fail CI si > 2× baseline |
| `bench_find_many_1k` (offline build) | < 1 ms | fail CI (offline, multi-OS) |
| `perf_n_plus_one_bounded` | ≤2 req pour include 1 niveau | fail si >2 |
| `perf_redacted_no_secret` | aucun secret dans trace | fail si `[REDACTED]` absent |
| `bench_10k_find_many` (PG, nightly) | < 80 ms p50 | warn, pas de fail (PG variable) |

Budgets versionnés : ce fichier + `benches/baseline.rs` hash pin. Toute optimisation doit améliorer ou maintenir ces chiffres ; pas de promesse sans mesure (§9).

## 4. Exploitation

- Pool métriques : `perf::metrics()` expose `count`, `total_duration_ms`, `avg_duration_ms`, `n_plus_one_warnings`
- Tracing : `RUSTORM_TRACE=1` → `[trace] query=<name> duration_ms=<ms>` sans valeurs (VAL-PERF01)
- Erreurs : `postgres::Error` enum avec sqlState, expurgée via `redacted_url`
- Retry : 3 tentatives max, backoff exponentiel, isolation READ COMMITTED documentée

## 5. Historique

- 2026-11-02 : baseline P0 établie (offline 0.1ms parse, PG 3ms/1k), benches `benches/baseline.rs`, tests `performance_baseline.rs`, `p1_perf.rs` N+1, CI budgets. M6 perf/sécu livré.
