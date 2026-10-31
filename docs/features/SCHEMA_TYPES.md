# Fiche de fonctionnalité — Types scalaires, enums et attributs de modèle

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SCHEMA-002` |
| Statut | **Spécification ; implémentation partielle P0** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §5.3](../CAHIER_DES_CHARGES.md#53-types-scalaires-attributs-de-modèle-et-enum-type-avéré--avant-génération) |
| Priorité | P0/P1 |

## 1. Résumé
Couvrir tous les types scalaires, enums et mapping vers PostgreSQL, avec attestations de type et projection des erreurs de base.

## 2. Contrat §5.3
- Scalaires P0 : `String`, `Boolean`, `Int` (int4), `BigInt` (int8), `Float`, `DateTime` (`timestamptz`), `Bytes` (`bytea`), `Json` (`jsonb`)
- Attributs : `@id`, `@unique`, `@default(autoincrement()|now()|val)`, `@map("nom")`, `@relation(name?, fields?, references?, onDelete?, onUpdate?)`, `@@id([champs])`, `@@unique([champs])`, `@@index([champs])`, `@@map("nom")` ; `@map` sur variante d’enum
- P1 : `Decimal` (`numeric(p,s)`), `UUID` si natif
- P2 : `@db.*`, vues, générateur SQL brut
- Mappage normalisé en IR : vérification types cohérents, défauts compatibles, enums utilisés seuls scalaires pour FK

## 3. État actuel
- Supportés : `Int→int4`, `BigInt→int8`, `Float→double precision|jsonb`, `Boolean→boolean`, `String→text`, nullabilité `?`, défaut `autoincrement()` → `serial/bigserial`/sequence, strings/nombres/booléens
- Validés : `@id`, `@@id`, `@unique`, `@map`, `@@map`, `@@index` mono-colonne, `@default` mono-val, `@relation` 1-1/1-N/N-N limité
- **Manques** :
  - `DateTime` → `timestamptz`, `@default(now())` avec `DEFAULT now()`, défauts expressions
  - `Bytes`→`bytea`, `Json`→`jsonb` (actuellement Float->jsonb comme hack)
  - `Decimal(p,s)` et `@db.Numeric/Decimal`, `@db.VarChar(N)`, etc. (P1) absents
  - `enum` et `@map` sur variante (dépendance SCHEMA_LANGUAGE)
  - `@@unique` composite (seulement `@@id` et `@@index`1c)
  - `@relation(onDelete/onUpdate: Cascade|Restrict|NoAction|SetNull|SetDefault)` — vérifié pour SetNull seulement
  - Type avéré : pas de propagation d’erreur `ColumnDecodeError` avec contexte type Rust ↔ PG

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-T01 | `DateTime @default(now())` implique `DEFAULT now()` |
| VAL-T02 | Type inconnu → diagnostic avec suggestion |
| VAL-T03 | `@default` incompatible → erreur (ex. autoincrement sur String) |
| VAL-T04 | `Json` et `Bytes` attestés, non implicites pour Float |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Types-01 | Schéma avec 8 scalaires + enum génère DDL attendu (types PG exacts) |
| AC-Types-02 | `@default(now())` sur DateTime : insertion sans valeur → timestamp serveur |
| AC-Types-03 | Fichier avec `Decimal @db.Decimal(10,2)` : P1 → `numeric(10,2)` sinon erreur claire |
| AC-Types-04 | Décodage colonne avec mauvais type → `TypeMismatch` avec champ+attendu+reçu |

## 6. Tests
- DDL : chaque type → type PG, chaque attribut → contrainte
- IR : incompatibilités (autoincrement non-id, now non-DateTime)
- Dépôt d’exemples AC §5.3

## 7. Risques
| Risque | Réduction |
|---|---|
| Json/Float conflit | Attester types, ne jamais inférer Json |
| Décimaux portables | P1 derrière feature flag + tests |

## 8. Livré / restant
- Livré : 5 scalaires + PK/unique/map/index1c/default simple/relation basique
- Restant : DateTime/Bytes/Json avancés, Decimal, enums, @@unique, onDelete/onUpdate complet, type avéré complet
