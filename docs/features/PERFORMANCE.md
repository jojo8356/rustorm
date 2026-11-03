# Fiche de fonctionnalité — Performance, observabilité et exploitation

| Métadonnée | Valeur |
|---|---|
| Identifiant | `PERF-001` |
| Statut | **Done — baseline outillée, N+1 borné, benches versionnés** |
| Version | 1.1 |
| Date | 2 novembre 2026 |
| Références | [Cahier §9](../CAHIER_DES_CHARGES.md#9-sécurité-performance-et-exploitation), `docs/PERFORMANCE_BASELINE.md`, `docs/PRISMA_DIFFERENTIAL_MATRIX.md` |
| Priorité | P1 |

## 1. Résumé
Instrumentation durée/rows/sql expurgé, borne N+1 ≤2 requêtes, benches 1k/10k, baselines chiffrées p50/p95/p99 + budgets CI — sans dégrader P0.

## 2. Contrat §9
- Instrumentation : `tracing` via `src/perf.rs` `instrumented_query` + `RUSTORM_TRACE=1`, durée, rows, sql tronqué/expurgé, span tx
- Borne N+1 : `include` 1 niveau → ≤2 requêtes (1 parent + 1 groupés) ; >1 niveau → N+1 documenté
- Benchmarks : `benches/baseline.rs` (criterion-like, `cargo bench`) pour 1k/10k, rapport reproductible
- Baselines : `docs/PERFORMANCE_BASELINE.md` chiffres p50/p95/p99 avant opti, overhead <15% vs brut
- Exploitation : métriques pool `deadpool`, erreurs typées `Error` expurgées, retries bornés

## 3. État actuel
- `src/perf.rs` : atomics `QUERY_COUNT/TOTAL_DURATION_MS/N_PLUS_ONE`, `record_query`, `metrics()`, `instrumented_query` avec `RUSTORM_TRACE`, N+1 warn >10
- `src/postgres.rs` : `AsyncPool` deadpool, `TransactionOptions` isolation/timeout/retry, `Error` 23505/23503/P0002, `redacted_url`
- `benches/baseline.rs` : parse/format/generate/query-build offline + PG 1k/10k, budgets anti-régression
- `docs/PERFORMANCE_BASELINE.md` : machine/PG/schéma/dataset/concurrence/pool, p50/p95/p99, budgets, overhead vs brut
- `tests/p1_perf.rs` : 4 tests offline `metrics/instrumented/N+1/tracing`
- `tests/performance_baseline.rs` : 8 tests offline budgets N+1 ≤2, expurgation, idempotent, overhead
- `tests/performance_postgres.rs` : 3 PG tests `find_many` 1k/10k < budgets, N+1 ≤2 (ignored offline)

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-PERF01 | Toute requête loggée expurgée (no secret in Debug/JSON) |
| VAL-PERF02 | N+1 mesuré par compteur requêtes réelles PG (≤2 pour 1 niveau) |

## 5. Critères
| ID | Critère | Preuve |
|---|---|---|
| AC-Perf-01 | N+1 compte requêtes réelles | `p1_perf::n_plus_one_detection_threshold` + `performance_postgres::include_bounded_2_requests` |
| AC-Perf-02 | Bench 10k `find_many` < 80 ms p50 (PG local) | `PERFORMANCE_BASELINE.md` 28 ms p50, budget <80 ms, `benches/baseline.rs` |
| AC-Perf-03 | Rapport baseline versionné | `docs/PERFORMANCE_BASELINE.md` v2026-11-02 + benches hash pin |

## 6. Livré / restant
- Livré : instrumentation tracing+metrics, N+1 2-req mesuré, benches offline/PG, baseline p50/p95/p99, budgets CI, docs exploitation — PERF-001 Done 2026-11-02, M6 perf/sécu
- Restant : optimisation future seulement si baseline en main
