# Fiche de fonctionnalité — Types composites (P2)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SCHEMA-COMPOSITES` |
| Statut | **P2 — à implémenter** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | Cahier §5 P2, Postgres composite docs |
| Priorité | P2 |

## 1. Résumé
Support `type Address { street String city String }` comme champ composite PostgreSQL, avec mapping vers struct Rust et création DDL `CREATE TYPE`.

## 2. Contrat
- `type Name { field Type }` — champs scalaires uniquement, pas de relations, pas de `@id`
- `model User { address Address }` → colonne `address` de type `"Address"` (CREATE TYPE)
- Validation : types composites existants, pas de cycles, noms distincts des modèles
- Génération : struct `Address` + conversions `ToSql/FromSql`

## 3. Critères
- Parse `type`, DDL contient `CREATE TYPE`, client struct généré
- Composite dans composite → erreur

## 4. Tests
- `composite_parsed`, `composite_ddl`, `composite_in_model`
