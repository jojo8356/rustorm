# Fiche de fonctionnalité — Migrations P2 : stratégies avancées

| Métadonnée | Valeur |
|---|---|
| Identifiant | `MIGRATION-P2` |
| Statut | **P2** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | Cahier §7 P2, §11 M6 |
| Priorité | P2 |

## 1. Résumé
Stratégies de déploiement `rolling`/`blue-green`, `migrate squash`, et support multi-connecteurs pour planification.

## 2. Contrat
- `rustorm migrate deploy --strategy rolling` : applique migration par migration avec vérification
- `rustorm migrate squash --name consolidated --from <id>` : fusionne migrations en une
- `--connector` pour planifier selon connecteur
- DDL généré adapté au connecteur (ex. CockroachDB `SERIAL` vs `IDENTITY`)

## 3. Critères
- `deploy --strategy` validé, `squash` produit SQL consolidé vérifié
- Plan refusé si stratégie incompatible connecteur

## 4. Tests
- `squash_migrations`, `strategy_rolling`
