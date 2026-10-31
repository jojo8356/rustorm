# Fiche de fonctionnalité — Studio & import SQL (P2)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `TOOLING-P2` |
| Statut | **P2** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | Cahier §3 P2 Outils, §8 |
| Priorité | P2 |

## 1. Résumé
`rustorm studio` (UI web locale) et `rustorm db import --sql` (SQL → RSM) pour P2.

## 2. Contrat
- `rustorm studio [--port 5542 --schema ...]` : lance serveur web local affichant modèles, enregistrements, requêteur (stub P2 renvoie page HTML statique + log)
- `rustorm db import --sql file.sql --to schema.rsm [--force]` : parse `CREATE TABLE` → `.rsm` (types PG→RSM, PK, FK, index)
- Pas de secret dans logs, serveurs bind 0.0.0.0 avec preview host allowlist

## 3. Critères
- `studio --help` doc, `import --sql` convertit exemple `CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT)`
- SQL avancé (triggers, vues) → warning

## 4. Tests
- `studio_starts`, `sql_import_generates_rsm`
