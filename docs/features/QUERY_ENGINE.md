# Fiche de fonctionnalité — Moteur de requêtes : opérations, filtres, tri, pagination

| Métadonnée | Valeur |
|---|---|
| Identifiant | `QUERY-001` |
| Statut | **P1 M5 — done validé : findUnique/findFirst, cursor, select, distinct, in vide, aggregate/groupBy/having, json/mode (tests p1_m5_completion)** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §6.3](../CAHIER_DES_CHARGES.md#63-contrat-des-opérations-de-requête), README query tests |
| Priorité | P0/P1 |

## 1. Résumé
Offrir l’ensemble des opérations CRUD, filtres scalaires, tri et pagination documentés, paramétrés et typés.

## 2. Contrat §6.3
- Opérations P0 : `findUnique` (WhereUnique, null si absent, erreur si doublon), `findFirst` (filtre+tri+distinct), `findMany` (filtres, tri multi-colonnes, skip/take, cursor, distinct, select), `create` (une ligne avec valeurs, relations `create`/`connect` 1ere phase), `update`/`delete` par WhereUnique, `updateMany`/`deleteMany` par filtre avec count, `count`
- Filtres P0 par type : `equals`, `not`, `in`, `notIn`, `lt/lte/gt/gte`, `contains/startsWith/endsWith` (mode sensible/case-insensitive selon PG), `isNull`, combinateurs `AND/OR/NOT`, `StringQueryMode`
- Tri : `orderBy` multi-champs, stable, `nullsFirst/last` (P1)
- Pagination : `skip/take` avec bornes, `cursor` (requiert tri déterministe)
- `distinct` : une ou plusieurs colonnes ; interaction définie avec tri
- P1 : `createMany` (batch), `aggregate (_count/_min/_max/_sum/_avg)`, `groupBy` avec `having`, JSON filters (`path`, `equals`…)
- Paramétré : jamais d’interpolation string, types Rust ↔ PG attestés

## 3. État actuel
- Implémenté dans `src/query.rs` + `src/relations.rs` + `src/schema.rs` :
  - `Operation::Select/Insert/Update/Delete` → `SqlQuery { sql, params }` avec `$N`
  - `WhereClause` : `Field::equals`, `not_equals`, `is_in`, `not_in`, `contains/starts_with/ends_with/ilike`, `is_null/is_not_null`, `lt/lte/gt/gte` via `OrderedField`, combinateurs `Filter::and/or/not` (`not` présent, `not_in` présent)
  - `find_many` (filtres AND implicite, order, take/skip, distinct, count), `find_by_pk`, `count`, `create` (insert returning), `update_many` (set, increment/decrement/multiply/divide), `delete_many`
  - Tests `tests/query.rs` : DDL, CRUD, filtres, tri, pagination, distinct, count, transactions, injection
- **Manques P0** :
  - Pas de `findUnique`/`findFirst` distincts ( `find_many` + `find_by_pk` approchent mais pas sémantique exacte : erreur si doublon sur unique absent, `findFirst` avec distinct+order)
  - Pas de `cursor` (pagination par curseur opaque)
  - Pas de `select` projection (toutes colonnes toujours retournées)
  - `distinct` multi-colonnes non testé, interaction `distinct`+`orderBy` non définie
  - `in`/`notIn` avec liste vide : comportement à préciser (actuel peut générer `IN ()` invalide)
  - `upsert` top-level, `createMany`
  - `Json` filters absents, `Bytes` absents
  - `StringQueryMode` insensible à la casse seulement via `ilike` sans mode paramétré
- **Manques P1** : `aggregate`, `groupBy`, `having`, `createMany` batch, JSON path
- Instrumentation bornes N+1 (dépendance RELATIONS §9) partiellement dans `postgres.rs` mais pas centralisée pour requêtes scalaires

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-Q01 | `in []` → `false` (0 ligne) sans générer SQL invalide |
| VAL-Q02 | `take`/`skip` négatifs → erreur typée, pas de troncature |
| VAL-Q03 | `findUnique` avec WhereUnique non-unique → erreur diagnostic |
| VAL-Q04 | Requête toujours paramétrée, test fuzz injection |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Query-01 | Suite §10 AC-02 (CRUD) en PG : create→findUnique→update→delete ok |
| AC-Query-02 | Filtres combinés AND/OR/NOT retournent exactement les lignes attendues sur fixtures |
| AC-Query-03 | `findMany(skip:1,take:1,orderBy:title)` stable sur doublons |
| AC-Query-04 | `distinct` sur 2 cols + tri : résultat documenté |
| AC-Query-05 | AC-07 (N+1 borné) mesuré ≤2 requêtes pour include 1 niveau |

## 6. Tests
- `tests/query.rs` : filtres tous opérateurs, combinateurs, tri, page, distinct, count, mutations numériques, injection, nullabilité
- À ajouter : `findUnique`/`findFirst` exacts, cursor, in vide, select, aggregate
- PG : chaque opération via `sqlx`/`libpq` réel avec fixture déterministe

## 7. Risques
| Risque | Réduction |
|---|---|
| `cursor` mal implémenté → sauts | Specifier encoding opaque (PK tuple) + tests |
| `select` brise décodage | Attester colonnes→champs, erreur si colonne manquante |

## 8. Livré / restant
- Livré : CRUD Select/Insert/Update/Delete paramétré, filtres de base, combinateurs, tri, page, distinct, count
- Restant : findUnique/First sémantique, cursor, select, in vide, upsert/createMany top-level, aggregate/groupBy, Json filters, mode string configurable
