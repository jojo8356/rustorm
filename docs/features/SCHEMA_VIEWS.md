# Fiche de fonctionnalité — Vues SQL (P2)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SCHEMA-VIEWS` |
| Statut | **P2 — à implémenter (cahier §3 P2)** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | [Cahier §5](../CAHIER_DES_CHARGES.md#5-langage-rsm), [Prisma views](https://pris.ly/d/views) |
| Priorité | P2 |

## 1. Résumé
Permettre de déclarer des vues SQL en lecture seule dans `.rsm` via `view Name { field Type @map("col") @@map("view_name") }`, sans génération DDL (lecture seule), mais avec client typé et introspection.

## 2. Contrat
- Syntaxe : `view ViewName { id Int @id @map("id") name String @@map("my_view") }`
- Vues sont read-only : pas de `create/update/delete`, seulement `findMany/findFirst`, pas de PK obligatoire, pas de `@default`, pas de relations own-side
- Validation : noms uniques, pas de `@@id` avec `Float`, @map autorisé, rejets si tentative d'écriture
- Génération : struct `ViewName::Record` + `find_many()` + `find_first()` seulement
- CLI `db pull` détecte `pg_views` et génère `view`

## 3. État cible
Parser `view`, IR `View { name, table, fields }`, validation, génération, introspection `pg_views`.

## 4. Critères
- `view` parse + format idempotent, `find_many` compile
- Tentative `create` sur vue → diagnostic

## 5. Tests
- `view` parse, `views_includes_views`, `view_read_only_no_create`
