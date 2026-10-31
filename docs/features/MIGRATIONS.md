# Fiche de fonctionnalité — Migrations : artéfacts, `migrate dev`, `deploy`, `status`, `resolve`, `baseline`

| Métadonnée | Valeur |
|---|---|
| Identifiant | `MIG-001` |
| Statut | **P1 M5 — done validé : create/plan/diff, deploy/status/resolve/baseline, composite FK DDL, split_sql, load (tests p1_m5_completion)** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §7.1](../CAHIER_DES_CHARGES.md#71-artéfacts-et-invariants-des-migrations), README migrations |
| Priorité | P0/P1 |

## 1. Résumé
Gérer les migrations versionnées `migrations/<timestamp>_<name>/migration.sql` avec verrou, historique, shadow DB, et commandes de réconciliation.

## 2. Contrat §7.1
- Artéfacts : `migrations/` + `migration_lock.toml` (ou équivalent), table `_rustorm_migrations`/`_prisma_migrations`, chaque migration `migration.sql` atomique
- `migrate dev` : diff schéma vs shadow DB → `CREATE TABLE/ADD COLUMN` additifs, noms FK/index déterministes, preview + `--create-only/--skip-generate` ; détecte destructif → demande `--force`
- `migrate deploy` : applique en prod, sans shadow, verrou `FOR UPDATE`, idempotent si déjà appliqué
- `migrate status` : liste appliquées/en attente/divergentes
- `migrate resolve --applied/--rolled-back "migration"` : marque sans SQL
- `migrate baseline` : marque l’état existant sans créer migration (import DB legacy)
- Shadow DB : base éphémère pour plan additif, détruite après, jamais de données prod
- DDL uniquement additif P0 ; renommage/suppression → `plan` ou P1

## 3. État actuel
- `src/migration_plan.rs` + `src/migrations.rs` : `MigrationPlan` (applied/pending), `Migration` structs, `migration_history` table, `plan` additif limité (create table, add column, index), `migrate`, `status`, `deploy` basiques, `baseline` partiel
- `src/main.rs` : `validate`, `generate`, `migrate create/status/deploy/plan` (pas `dev/resolve/baseline` complets)
- **Manques P0** :
  - `migrate dev` avec shadow DB éphémère absent (plan actuel compare en mémoire, pas de shadow PG réel)
  - `migrate deploy` sans verrou robuste ni idempotence complète
  - `migrate resolve` absent
  - `migrate baseline` incomplet (pas de marquage historique avec checksum)
  - Noms FK/index non déterministes dans tous les cas
  - Détection destructive limitée
  - `migration_lock.toml` absent (utilise table seulement)
  - Destructif P1 non géré (renommage, suppression → seulement `plan` textuel)

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-M01 | Migration jamais éditée après apply → checksum mismatch → erreur |
| VAL-M02 | `dev` ne touche jamais DB prod, seulement shadow |
| VAL-M03 | `deploy` verrouillé, rejouable |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Mig-01 | Cycle `dev`×3 (create, add col, add index) → `deploy` idempotent |
| AC-Mig-02 | `status` affiche pending/applied divergent |
| AC-Mig-03 | `resolve --rolled-back` répare historique sans SQL |
| AC-Mig-04 | `baseline` sur DB legacy → migrations suivantes ok |
| AC-Mig-05 | Destructif sans `--force` → refuse |

## 6. Tests
- `tests/migrations.rs` en mémoire + `tests/migrations_postgres.rs` disposable : create, plan, deploy, renommage rejeté
- À ajouter : shadow DB réel, resolve, baseline, verrou, idempotence deploy, destructive guard

## 7. Risques
| Risque | Réduction |
|---|---|
| Shadow laisse résidu | Toujours `DROP DATABASE` en finally |
| Baseline casse historique | Vérifier checksum, documenter |

## 8. Livré / restant
- Livré : plan additif en mémoire, migrate/status/deploy partiels, tests PG
- Restant : shadow DB réel, dev/resolve/baseline complets, verrou, migration_lock.toml, détection destructive robuste
