# Fiche de fonctionnalité — Performance, observabilité et exploitation

| Métadonnée | Valeur |
|---|---|
| Identifiant | `PERF-001` |
| Statut | **Non formellement outillé ; métriques de base absentes** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §9](../CAHIER_DES_CHARGES.md#9-sécurité-performance-et-exploitation), §14 jalons |
| Priorité | P1/P2 |

## 1. Résumé
Instrumenter requêtes, borner N+1, mesurer et documenter performances, sans dégrader P0.

## 2. Contrat §9
- Instrumentation : `tracing` ou `log` avec durée, rows, sql tronqué/expurgé, span transaction
- Borne N+1 : `include` 1 niveau → ≤2 requêtes (1 parent + 1 enfants groupés) ; plus → documenté
- Benchmarks : `cargo bench` ou `criterion` pour 1k/10k lignes, avec rapport reproductible
- Baselines : chiffres de référence avant optimisation, pas de promesse sans mesure
- Exploitation : métriques pool, erreurs, retries

## 3. État actuel
- Aucune instrumentation `tracing`, pas de bench `criterion`, pas de baseline chiffrée
- `relations.rs` `include` fait 2 requêtes (parent+children) — conforme mais non mesuré formellement
- `postgres.rs` n’expose pas durée/rows

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-PERF01 | Toute requête loggée expurgée |
| VAL-PERF02 | N+1 mesuré par test compteur requêtes |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Perf-01 | Test N+1 compte requêtes réelles PG |
| AC-Perf-02 | Bench 10k `find_many` < 1s sur PG local (à établir) |
| AC-Perf-03 | Rapport baseline versionné |

## 6. Livré / restant
- Livré : comportement N+1 2-requêtes implicite
- Restant : instrumentation, benches, baselines, doc exploitation
